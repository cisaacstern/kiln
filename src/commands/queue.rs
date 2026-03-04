use anyhow::{Result, bail};
use chrono::Utc;
use std::path::{Path, PathBuf};

use crate::state::{self, Status, TaskState};
use crate::tsk;

pub fn run(name: &str, prompt_file: Option<&Path>) -> Result<()> {
    // Validate task name to prevent path traversal
    state::validate_task_name(name)?;

    let resolved_prompt = match prompt_file {
        Some(pf) => resolve_explicit_prompt(pf)?,
        None => resolve_plan_prompt(name)?,
    };

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
    let (tsk_id, tsk_branch) = tsk::add_task(name, &resolved_prompt)?;

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

fn resolve_explicit_prompt(prompt_file: &Path) -> Result<PathBuf> {
    if !prompt_file.exists() {
        bail!("Prompt file not found: {}", prompt_file.display());
    }
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
    Ok(canonical_prompt)
}

fn resolve_plan_prompt(name: &str) -> Result<PathBuf> {
    let mut plans = state::read_plans()?;

    let entry = plans
        .get(name)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "No plan file found for '{}'. Write a plan first, or use --prompt-file.",
                name
            )
        })?
        .clone();

    let plans_dir = state::plans_dir()?;

    // If plan_file is already set, use it
    if let Some(ref filename) = entry.plan_file {
        let path = plans_dir.join(filename);
        if !path.exists() {
            bail!(
                "Plan file '{}' no longer exists at {}",
                filename,
                path.display()
            );
        }
        return Ok(path);
    }

    // Scan for newest .md file created after the plan's created_at that isn't claimed
    let claimed: std::collections::HashSet<String> = plans
        .values()
        .filter_map(|e| e.plan_file.clone())
        .collect();

    let mut candidates: Vec<(String, std::time::SystemTime)> = Vec::new();
    for entry_result in std::fs::read_dir(&plans_dir)? {
        let dir_entry = entry_result?;
        let file_name = dir_entry.file_name().to_string_lossy().to_string();
        if !file_name.ends_with(".md") {
            continue;
        }
        if claimed.contains(&file_name) {
            continue;
        }
        let metadata = dir_entry.metadata()?;
        if let Ok(modified) = metadata.modified() {
            let modified_chrono: chrono::DateTime<chrono::Utc> = modified.into();
            if modified_chrono > entry.created_at {
                candidates.push((file_name, modified));
            }
        }
    }

    if candidates.is_empty() {
        bail!(
            "No plan file found for '{}'. Write a plan first, or use --prompt-file.",
            name
        );
    }

    // Pick the newest
    candidates.sort_by(|a, b| b.1.cmp(&a.1));
    let chosen = candidates[0].0.clone();

    // Update plans.json with resolved filename
    if let Some(plan) = plans.get_mut(name) {
        plan.plan_file = Some(chosen.clone());
    }
    state::write_plans(&plans)?;

    let path = plans_dir.join(&chosen);
    println!("Auto-resolved plan file: {}", chosen);
    Ok(path)
}
