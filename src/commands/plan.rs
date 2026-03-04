use anyhow::{Context, Result, bail};
use chrono::Utc;

use crate::state::{self, PlanEntry};
use crate::tmux;

/// Get all Claude panes (kiln_name matching claude-*), sorted by kiln_name
fn claude_panes() -> Result<Vec<tmux::PaneInfo>> {
    let all = tmux::list_session_panes()?;
    let mut claude: Vec<_> = all
        .into_iter()
        .filter(|p| p.kiln_name.starts_with("claude-"))
        .collect();
    claude.sort_by(|a, b| a.kiln_name.cmp(&b.kiln_name));
    Ok(claude)
}

/// Find the pane currently in window 0 (the active Claude pane)
fn active_claude_pane(panes: &[tmux::PaneInfo]) -> Option<usize> {
    panes.iter().position(|p| p.window_index == "0")
}

pub fn run_new(name: &str, dir: Option<&std::path::Path>) -> Result<()> {
    state::validate_task_name(name)?;

    let mut plans = state::read_plans()?;
    if plans.contains_key(name) {
        bail!("Plan '{}' already registered. Use a different name.", name);
    }

    // Register the plan
    plans.insert(
        name.to_string(),
        PlanEntry {
            created_at: Utc::now(),
            plan_file: None,
        },
    );
    state::write_plans(&plans)?;

    // Determine working directory
    let project_root = state::find_project_root()?;
    let work_dir = dir.unwrap_or(&project_root);
    let work_dir_str = work_dir
        .to_str()
        .context("Working directory path is not valid UTF-8")?;

    // Create new Claude pane in parking window
    let new_pane_id = tmux::create_parked_pane(work_dir_str)?;
    tmux::set_pane_title(&new_pane_id, &format!("claude-{name}"))?;
    tmux::set_pane_option(&new_pane_id, &format!("claude-{name}"))?;

    // Swap the new pane with the active Claude pane in window 0
    let current_claude = tmux::active_claude_pane_id()?;
    if current_claude != new_pane_id {
        tmux::swap_pane(&new_pane_id, &current_claude)?;
    }

    // Ensure client stays on window 0
    tmux::select_window("kiln:0")?;

    println!(
        "Plan '{}' registered. Claude pane claude-{} is now active.",
        name, name
    );
    println!("Write your plan in Claude, then run 'kiln queue {}'.", name);
    Ok(())
}

pub fn run_next() -> Result<()> {
    let panes = claude_panes()?;
    if panes.len() < 2 {
        bail!("Only one Claude pane exists. Use 'kiln plan new' to create more.");
    }

    let current = active_claude_pane(&panes).context("No active Claude pane in window 0")?;
    let next = (current + 1) % panes.len();

    let active_id = tmux::active_claude_pane_id()?;
    tmux::swap_pane(&panes[next].pane_id, &active_id)?;

    println!("Switched to {}", panes[next].kiln_name);
    Ok(())
}

pub fn run_prev() -> Result<()> {
    let panes = claude_panes()?;
    if panes.len() < 2 {
        bail!("Only one Claude pane exists. Use 'kiln plan new' to create more.");
    }

    let current = active_claude_pane(&panes).context("No active Claude pane in window 0")?;
    let prev = if current == 0 {
        panes.len() - 1
    } else {
        current - 1
    };

    let active_id = tmux::active_claude_pane_id()?;
    tmux::swap_pane(&panes[prev].pane_id, &active_id)?;

    println!("Switched to {}", panes[prev].kiln_name);
    Ok(())
}

pub fn run_list() -> Result<()> {
    let panes = claude_panes()?;
    if panes.is_empty() {
        println!("No Claude panes found.");
        return Ok(());
    }

    let active_idx = active_claude_pane(&panes);

    for (i, pane) in panes.iter().enumerate() {
        let marker = if Some(i) == active_idx { " *" } else { "" };
        println!(
            "  {} (window {}){marker}",
            pane.kiln_name, pane.window_index
        );
    }
    Ok(())
}
