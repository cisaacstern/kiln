use anyhow::{Context, Result, bail};

use crate::state;
use crate::tmux;

/// Required external dependencies
const REQUIRED_DEPS: &[&str] = &["tmux", "tsk", "npx", "git"];

pub fn run() -> Result<()> {
    // Check dependencies
    check_dependencies()?;

    // Check if session already exists
    if tmux::session_exists() {
        println!("kiln session already exists. Attaching...");
        return tmux::attach_session();
    }

    let project_dir = state::find_project_root()?;
    let project_dir_str = project_dir
        .to_str()
        .context("Project root path is not valid UTF-8")?;

    println!("Starting kiln session...");

    // Create tmux session with 4-pane layout
    tmux::create_session(project_dir_str)?;

    // Pane 1: tsk server
    tmux::send_keys(1, "tsk server start")?;

    // Pane 2: kiln status watcher
    tmux::send_keys(2, "kiln status --daemon")?;

    // Pane 3: claude code
    tmux::send_keys(3, "claude")?;

    // Title the Claude pane
    let pane_id = tmux::get_pane_id("kiln:0.3")?;
    tmux::set_pane_title(&pane_id, "claude-0")?;

    // Create sidebar pane to the left of the Claude pane
    let sidebar_pane_id = tmux::split_pane_horizontal(&pane_id, 25, project_dir_str, true)?;
    tmux::set_pane_title(&sidebar_pane_id, "sidebar")?;
    tmux::send_keys_to_pane(&sidebar_pane_id, "kiln sidebar --daemon")?;

    // Focus the main terminal pane
    tmux::select_pane(0)?;

    // Set initial window name
    tmux::set_window_name("kiln")?;

    println!("kiln session started. Attaching...");
    tmux::attach_session()
}

fn check_dependencies() -> Result<()> {
    let mut missing = Vec::new();
    for dep in REQUIRED_DEPS {
        if which::which(dep).is_err() {
            missing.push(*dep);
        }
    }
    if !missing.is_empty() {
        bail!(
            "Missing required dependencies: {}.\nInstall them before running kiln start.",
            missing.join(", ")
        );
    }
    Ok(())
}
