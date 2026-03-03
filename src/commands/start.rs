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

    // Create tmux session with 5-pane layout
    tmux::create_session(project_dir_str)?;

    // Pane 1: tsk server
    tmux::send_keys(1, "tsk serve")?;

    // Pane 2: tsk list watch
    tmux::send_keys(2, "watch -n2 tsk list")?;

    // Pane 3: kiln status watcher
    tmux::send_keys(3, "kiln status --daemon")?;

    // Pane 4: claude code
    tmux::send_keys(4, "claude")?;

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
