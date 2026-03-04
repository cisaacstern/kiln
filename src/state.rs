use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Queued,
    Running,
    PendingReview,
    InReview,
    ChangesRequested,
    Approved,
    Failed,
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Status::Queued => write!(f, "QUEUED"),
            Status::Running => write!(f, "RUNNING"),
            Status::PendingReview => write!(f, "PENDING_REVIEW"),
            Status::InReview => write!(f, "IN_REVIEW"),
            Status::ChangesRequested => write!(f, "CHANGES_REQUESTED"),
            Status::Approved => write!(f, "APPROVED"),
            Status::Failed => write!(f, "FAILED"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskState {
    pub status: Status,
    pub tsk_id: String,
    pub tsk_branch: String,
    pub base_ref: String,
    pub base_commit: String,
    pub revision: u32,
    pub updated_at: DateTime<Utc>,
}

pub type StateMap = HashMap<String, TaskState>;

/// Find the project root by looking for .git directory
pub fn find_project_root() -> Result<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .context("Failed to run git")?;
    if !output.status.success() {
        bail!("Not in a git repository");
    }
    let root = String::from_utf8(output.stdout)?.trim().to_string();
    Ok(PathBuf::from(root))
}

/// Get the .kiln directory path, creating it if needed
pub fn kiln_dir() -> Result<PathBuf> {
    let root = find_project_root()?;
    let dir = root.join(".kiln");
    if !dir.exists() {
        fs::create_dir_all(&dir).context("Failed to create .kiln directory")?;
    }
    let plans_dir = dir.join("plans");
    if !plans_dir.exists() {
        fs::create_dir_all(&plans_dir).context("Failed to create .kiln/plans directory")?;
    }
    Ok(dir)
}

fn state_file_path() -> Result<PathBuf> {
    Ok(kiln_dir()?.join("state.json"))
}

/// Acquire an advisory lock on the state lock file.
/// Returns the lock file handle (lock released on drop).
fn lock_state(exclusive: bool) -> Result<fs::File> {
    let lock_path = kiln_dir()?.join("state.lock");
    let lock_file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .context("Failed to open state lock file")?;
    if exclusive {
        lock_file
            .lock_exclusive()
            .context("Failed to acquire exclusive state lock")?;
    } else {
        lock_file
            .lock_shared()
            .context("Failed to acquire shared state lock")?;
    }
    Ok(lock_file)
}

/// Read the state file, returning an empty map if it doesn't exist
pub fn read_state() -> Result<StateMap> {
    let _lock = lock_state(false)?;
    let path = state_file_path()?;
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let contents = fs::read_to_string(&path).context("Failed to read state file")?;
    let state: StateMap = serde_json::from_str(&contents).context("Failed to parse state file")?;
    Ok(state)
}

/// Write the state file atomically (write to temp file, then rename)
pub fn write_state(state: &StateMap) -> Result<()> {
    let _lock = lock_state(true)?;
    let path = state_file_path()?;
    let dir = path.parent().context("state file path has no parent")?;
    let temp_path = dir.join("state.json.tmp");
    let contents = serde_json::to_string_pretty(state)?;
    fs::write(&temp_path, &contents).context("Failed to write temp state file")?;

    // Set restrictive permissions before renaming into place
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&temp_path, perms)
            .context("Failed to set state file permissions")?;
    }

    fs::rename(&temp_path, &path).context("Failed to rename temp state file")?;
    Ok(())
}

/// Get a task by name, returning an error if not found
pub fn get_task(state: &StateMap, name: &str) -> Result<TaskState> {
    state
        .get(name)
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("Task '{}' not found in kiln state", name))
}

/// Update a task's status and timestamp
pub fn update_task_status(name: &str, new_status: Status) -> Result<()> {
    let mut state = read_state()?;
    let task = state
        .get_mut(name)
        .ok_or_else(|| anyhow::anyhow!("Task '{}' not found", name))?;
    task.status = new_status;
    task.updated_at = Utc::now();
    write_state(&state)
}

/// Get the current git HEAD commit SHA
pub fn current_commit() -> Result<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .context("Failed to run git rev-parse")?;
    if !output.status.success() {
        bail!("Failed to get current commit");
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

/// Get the current git branch name
pub fn current_branch() -> Result<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .context("Failed to get current branch")?;
    if !output.status.success() {
        bail!("Failed to get current branch");
    }
    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

/// Validate that a task name contains only safe characters (alphanumeric, hyphens, underscores).
/// Rejects path separators and special characters to prevent path traversal.
pub fn validate_task_name(name: &str) -> Result<()> {
    if name.is_empty() {
        bail!("Task name must not be empty");
    }
    let re = regex::Regex::new(r"^[a-zA-Z0-9_-]+$").unwrap();
    if !re.is_match(name) {
        bail!(
            "Invalid task name '{}': must contain only alphanumeric characters, hyphens, and underscores",
            name
        );
    }
    Ok(())
}

/// Validate that a string looks like a safe git ref name.
/// Rejects shell metacharacters and other potentially dangerous characters.
pub fn validate_git_ref(ref_name: &str) -> Result<()> {
    if ref_name.is_empty() {
        bail!("Git ref name must not be empty");
    }
    let re = regex::Regex::new(r"^[a-zA-Z0-9/_.\-]+$").unwrap();
    if !re.is_match(ref_name) {
        bail!(
            "Invalid git ref '{}': contains disallowed characters",
            ref_name
        );
    }
    Ok(())
}

/// Count tasks by status
pub fn count_by_status(state: &StateMap, status: Status) -> usize {
    state.values().filter(|t| t.status == status).count()
}

// --- Plan registry ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanEntry {
    pub created_at: DateTime<Utc>,
    pub plan_file: Option<String>,
}

pub type PlanMap = HashMap<String, PlanEntry>;

fn plans_file_path() -> Result<PathBuf> {
    Ok(kiln_dir()?.join("plans.json"))
}

fn lock_plans(exclusive: bool) -> Result<fs::File> {
    let lock_path = kiln_dir()?.join("plans.lock");
    let lock_file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .context("Failed to open plans lock file")?;
    if exclusive {
        lock_file
            .lock_exclusive()
            .context("Failed to acquire exclusive plans lock")?;
    } else {
        lock_file
            .lock_shared()
            .context("Failed to acquire shared plans lock")?;
    }
    Ok(lock_file)
}

pub fn read_plans() -> Result<PlanMap> {
    let _lock = lock_plans(false)?;
    let path = plans_file_path()?;
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let contents = fs::read_to_string(&path).context("Failed to read plans file")?;
    let plans: PlanMap = serde_json::from_str(&contents).context("Failed to parse plans file")?;
    Ok(plans)
}

pub fn write_plans(plans: &PlanMap) -> Result<()> {
    let _lock = lock_plans(true)?;
    let path = plans_file_path()?;
    let dir = path.parent().context("plans file path has no parent")?;
    let temp_path = dir.join("plans.json.tmp");
    let contents = serde_json::to_string_pretty(plans)?;
    fs::write(&temp_path, &contents).context("Failed to write temp plans file")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&temp_path, perms)
            .context("Failed to set plans file permissions")?;
    }

    fs::rename(&temp_path, &path).context("Failed to rename temp plans file")?;
    Ok(())
}

/// Get the plans directory path
pub fn plans_dir() -> Result<PathBuf> {
    Ok(kiln_dir()?.join("plans"))
}

// --- Pane status ---

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaneStatus {
    Working,
    Waiting,
    Done,
    Unknown,
}

impl PaneStatus {
    pub fn icon(&self) -> &'static str {
        match self {
            PaneStatus::Working => "🤖",
            PaneStatus::Waiting => "✋",
            PaneStatus::Done => "✅",
            PaneStatus::Unknown => "⚪",
        }
    }
}

impl std::str::FromStr for PaneStatus {
    type Err = anyhow::Error;
    fn from_str(s: &str) -> Result<Self> {
        match s {
            "working" => Ok(PaneStatus::Working),
            "waiting" => Ok(PaneStatus::Waiting),
            "done" => Ok(PaneStatus::Done),
            "unknown" => Ok(PaneStatus::Unknown),
            _ => bail!("Invalid pane status '{}': expected working, waiting, done, or unknown", s),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneStatusEntry {
    pub status: PaneStatus,
    pub updated_at: DateTime<Utc>,
}

pub type PaneStatusMap = HashMap<String, PaneStatusEntry>;

fn pane_status_file_path() -> Result<PathBuf> {
    Ok(kiln_dir()?.join("pane-status.json"))
}

fn lock_pane_status(exclusive: bool) -> Result<fs::File> {
    let lock_path = kiln_dir()?.join("pane-status.lock");
    let lock_file = fs::OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
        .context("Failed to open pane-status lock file")?;
    if exclusive {
        lock_file
            .lock_exclusive()
            .context("Failed to acquire exclusive pane-status lock")?;
    } else {
        lock_file
            .lock_shared()
            .context("Failed to acquire shared pane-status lock")?;
    }
    Ok(lock_file)
}

pub fn read_pane_status() -> Result<PaneStatusMap> {
    let _lock = lock_pane_status(false)?;
    let path = pane_status_file_path()?;
    if !path.exists() {
        return Ok(HashMap::new());
    }
    let contents = fs::read_to_string(&path).context("Failed to read pane-status file")?;
    let map: PaneStatusMap =
        serde_json::from_str(&contents).context("Failed to parse pane-status file")?;
    Ok(map)
}

pub fn write_pane_status(map: &PaneStatusMap) -> Result<()> {
    let _lock = lock_pane_status(true)?;
    let path = pane_status_file_path()?;
    let dir = path.parent().context("pane-status file path has no parent")?;
    let temp_path = dir.join("pane-status.json.tmp");
    let contents = serde_json::to_string_pretty(map)?;
    fs::write(&temp_path, &contents).context("Failed to write temp pane-status file")?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o600);
        std::fs::set_permissions(&temp_path, perms)
            .context("Failed to set pane-status file permissions")?;
    }

    fs::rename(&temp_path, &path).context("Failed to rename temp pane-status file")?;
    Ok(())
}
