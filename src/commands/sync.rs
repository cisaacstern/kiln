use anyhow::{Result, bail};
use chrono::Utc;

use crate::state::{self, Status, TaskState};
use crate::tsk;

pub fn run(name: &str, base_ref: &str) -> Result<()> {
    state::validate_task_name(name)?;
    state::validate_git_ref(base_ref)?;

    let mut state_map = state::read_state()?;

    // Reject if already actively tracked
    if let Some(existing) = state_map.get(name) {
        if !matches!(existing.status, Status::Approved | Status::Failed) {
            bail!(
                "Task '{}' already tracked (status: {})",
                name,
                existing.status
            );
        }
    }

    // Look up in tsk
    let tsk_task = tsk::find_task_by_name(name)?
        .ok_or_else(|| anyhow::anyhow!("Task '{}' not found in tsk", name))?;

    if tsk_task.branch.is_empty() {
        bail!("tsk task '{}' has no branch", name);
    }

    state::validate_git_ref(&tsk_task.branch)?;

    // Verify branch exists locally
    let branch_check = std::process::Command::new("git")
        .args(["rev-parse", "--verify", &tsk_task.branch])
        .output()?;
    if !branch_check.status.success() {
        bail!(
            "Branch '{}' does not exist locally",
            tsk_task.branch
        );
    }

    // Get merge-base for base_commit
    let merge_base_output = std::process::Command::new("git")
        .args(["merge-base", base_ref, &tsk_task.branch])
        .output()?;
    if !merge_base_output.status.success() {
        bail!(
            "Could not find merge-base between '{}' and '{}'",
            base_ref,
            tsk_task.branch
        );
    }
    let base_commit = String::from_utf8_lossy(&merge_base_output.stdout)
        .trim()
        .to_string();

    // Map tsk status -> kiln status
    let status = match tsk_task.status.to_uppercase().as_str() {
        "COMPLETE" | "DONE" => Status::PendingReview,
        "RUNNING" | "IN_PROGRESS" => Status::Running,
        "FAILED" | "ERROR" => Status::Failed,
        _ => Status::Queued,
    };

    let revision = if let Some(existing) = state_map.get(name) {
        existing.revision + 1
    } else {
        1
    };

    state_map.insert(
        name.to_string(),
        TaskState {
            status,
            tsk_id: tsk_task.id,
            tsk_branch: tsk_task.branch.clone(),
            base_ref: base_ref.to_string(),
            base_commit,
            revision,
            updated_at: Utc::now(),
        },
    );
    state::write_state(&state_map)?;

    println!(
        "Synced task '{}': {} on branch {}",
        name, status, tsk_task.branch
    );
    Ok(())
}
