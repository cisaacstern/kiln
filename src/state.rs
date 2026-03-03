use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
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
    Ok(dir)
}

fn state_file_path() -> Result<PathBuf> {
    Ok(kiln_dir()?.join("state.json"))
}

/// Read the state file, returning an empty map if it doesn't exist
pub fn read_state() -> Result<StateMap> {
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
    let path = state_file_path()?;
    let dir = path.parent().unwrap();
    let temp_path = dir.join("state.json.tmp");
    let contents = serde_json::to_string_pretty(state)?;
    fs::write(&temp_path, &contents).context("Failed to write temp state file")?;
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

/// Count tasks by status
pub fn count_by_status(state: &StateMap, status: Status) -> usize {
    state.values().filter(|t| t.status == status).count()
}
