use anyhow::{Context, Result, bail};
use chrono::Utc;

use crate::state::{self, PlanEntry};
use crate::tmux;

/// Get all Claude panes (titles matching claude-*), sorted by title
fn claude_panes() -> Result<Vec<tmux::PaneInfo>> {
    let all = tmux::list_session_panes()?;
    let mut claude: Vec<_> = all
        .into_iter()
        .filter(|p| p.title.starts_with("claude-"))
        .collect();
    claude.sort_by(|a, b| a.title.cmp(&b.title));
    Ok(claude)
}

/// Find the pane currently in window 0 (the active Claude pane)
fn active_claude_pane(panes: &[tmux::PaneInfo]) -> Option<usize> {
    panes.iter().position(|p| p.window_index == "0")
}

/// Get the next claude-N index
fn next_claude_index(panes: &[tmux::PaneInfo]) -> u32 {
    panes
        .iter()
        .filter_map(|p| p.title.strip_prefix("claude-")?.parse::<u32>().ok())
        .max()
        .map(|n| n + 1)
        .unwrap_or(0)
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

    // Create new Claude pane
    let existing = claude_panes()?;
    let idx = next_claude_index(&existing);
    let new_pane_id = tmux::new_hidden_window(work_dir_str)?;
    tmux::set_pane_title(&new_pane_id, &format!("claude-{idx}"))?;

    // Swap the new pane into pane 3 of window 0
    let current_pane3 = tmux::get_pane_id("kiln:0.3")?;
    if current_pane3 != new_pane_id {
        tmux::swap_pane(&new_pane_id, &current_pane3)?;
    }

    println!(
        "Plan '{}' registered. Claude pane claude-{} is now active.",
        name, idx
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

    let current_pane3 = tmux::get_pane_id("kiln:0.3")?;
    tmux::swap_pane(&panes[next].pane_id, &current_pane3)?;

    println!("Switched to {}", panes[next].title);
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

    let current_pane3 = tmux::get_pane_id("kiln:0.3")?;
    tmux::swap_pane(&panes[prev].pane_id, &current_pane3)?;

    println!("Switched to {}", panes[prev].title);
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
            pane.title, pane.window_index
        );
    }
    Ok(())
}
