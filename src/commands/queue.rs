use anyhow::{Result, bail};
use chrono::Utc;
use std::path::Path;

use crate::state::{self, Status, TaskState};
use crate::tsk;

pub fn run(name: &str, prompt_file: &Path) -> Result<()> {
    // Validate task name to prevent path traversal
    state::validate_task_name(name)?;

    if !prompt_file.exists() {
        bail!("Prompt file not found: {}", prompt_file.display());
    }

    // Canonicalize the prompt file path and verify it's within the project root
    let canonical_prompt = prompt_file
        .canonicalize()
        .map_err(|e| anyhow::anyhow!("Failed to resolve prompt file path: {}", e))?;
    let project_root = state::find_project_root()?;
    if !canonical_prompt.starts_with(&project_root) {
        bail!(
            "Prompt file must be within the project directory ({})",
            project_root.display()
        );
    }

    let mut state_map = state::read_state()?;

    // Reject duplicate active task names
    if let Some(existing) = state_map.get(name) {
        if !matches!(existing.status, Status::Approved | Status::Failed) {
            bail!(
                "Task '{}' already exists (status: {}). Use a different name.",
                name,
                existing.status
            );
        }
    }

    // Capture base ref info before queuing
    let base_ref = state::current_branch()?;
    let base_commit = state::current_commit()?;

    // Check if this is a re-queue (revision increment)
    let revision = if let Some(existing) = state_map.get(name) {
        existing.revision + 1
    } else {
        1
    };

    println!("Queuing task '{name}' (revision {revision})...");

    // Call tsk add
    let (tsk_id, tsk_branch) = tsk::add_task(name, prompt_file)?;

    println!("Task queued: id={tsk_id}, branch={tsk_branch}");

    // Update state
    let task_state = TaskState {
        status: Status::Queued,
        tsk_id,
        tsk_branch,
        base_ref,
        base_commit,
        revision,
        updated_at: Utc::now(),
    };

    state_map.insert(name.to_string(), task_state);
    state::write_state(&state_map)?;

    println!("State saved to .kiln/state.json");
    Ok(())
}
