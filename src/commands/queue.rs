use anyhow::{Result, bail};
use chrono::Utc;
use std::path::Path;

use crate::state::{self, Status, TaskState};
use crate::tsk;

pub fn run(name: &str, prompt_file: &Path) -> Result<()> {
    if !prompt_file.exists() {
        bail!("Prompt file not found: {}", prompt_file.display());
    }

    let mut state_map = state::read_state()?;

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
