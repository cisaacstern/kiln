use anyhow::{Context, Result};
use std::process::Command;

const SESSION_NAME: &str = "kiln";

/// Check if we're currently inside a tmux session
pub fn is_inside_tmux() -> bool {
    std::env::var("TMUX").is_ok()
}

/// Check if the kiln tmux session exists
pub fn session_exists() -> bool {
    Command::new("tmux")
        .args(["has-session", "-t", SESSION_NAME])
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Create the kiln tmux session with 5-pane layout
pub fn create_session(project_dir: &str) -> Result<()> {
    // Create new detached session (pane 0 = main terminal)
    run_tmux(&[
        "new-session",
        "-d",
        "-s",
        SESSION_NAME,
        "-c",
        project_dir,
    ])?;

    // Split to create bottom area (pane 1)
    // Main terminal keeps ~40% height
    run_tmux(&[
        "split-window",
        "-t",
        &format!("{SESSION_NAME}:0.0"),
        "-v",
        "-p",
        "60",
        "-c",
        project_dir,
    ])?;

    // Split pane 1 horizontally to create pane 2 (tsk list)
    run_tmux(&[
        "split-window",
        "-t",
        &format!("{SESSION_NAME}:0.1"),
        "-h",
        "-p",
        "50",
        "-c",
        project_dir,
    ])?;

    // Split pane 1 vertically to create bottom-left row (pane 3 = devloop status)
    run_tmux(&[
        "split-window",
        "-t",
        &format!("{SESSION_NAME}:0.1"),
        "-v",
        "-p",
        "50",
        "-c",
        project_dir,
    ])?;

    // Split pane 2 vertically to create bottom-right (pane 4 = claude code)
    run_tmux(&[
        "split-window",
        "-t",
        &format!("{SESSION_NAME}:0.2"),
        "-v",
        "-p",
        "50",
        "-c",
        project_dir,
    ])?;

    Ok(())
}

/// Send a command to a specific pane.
/// Safety: `keys` must never contain user-controlled input, as it is passed
/// directly to tmux send-keys without sanitization.
pub(crate) fn send_keys(pane: u32, keys: &str) -> Result<()> {
    run_tmux(&[
        "send-keys",
        "-t",
        &format!("{SESSION_NAME}:0.{pane}"),
        keys,
        "Enter",
    ])
}

/// Update the tmux window name
pub fn set_window_name(name: &str) -> Result<()> {
    // Silently skip if not in tmux or session doesn't exist
    if !session_exists() {
        return Ok(());
    }
    run_tmux(&[
        "rename-window",
        "-t",
        &format!("{SESSION_NAME}:0"),
        name,
    ])
}

/// Build the window name with emoji indicators
pub fn build_window_name(pending_review: usize, changes_requested: usize) -> String {
    let mut name = SESSION_NAME.to_string();
    if pending_review > 0 {
        name.push_str(&format!(" 👀{pending_review}"));
    }
    if changes_requested > 0 {
        name.push_str(&format!(" 🔄{changes_requested}"));
    }
    name
}

/// Attach to the kiln tmux session
pub fn attach_session() -> Result<()> {
    if is_inside_tmux() {
        // Switch client if already in tmux
        run_tmux(&["switch-client", "-t", SESSION_NAME])
    } else {
        run_tmux(&["attach-session", "-t", SESSION_NAME])
    }
}

/// Kill the kiln tmux session
pub fn kill_session() -> Result<()> {
    run_tmux(&["kill-session", "-t", SESSION_NAME])
}

/// Select/focus a specific pane
pub fn select_pane(pane: u32) -> Result<()> {
    run_tmux(&[
        "select-pane",
        "-t",
        &format!("{SESSION_NAME}:0.{pane}"),
    ])
}

fn run_tmux(args: &[&str]) -> Result<()> {
    let output = Command::new("tmux")
        .args(args)
        .output()
        .context("Failed to run tmux")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("tmux command failed: {}", stderr);
    }
    Ok(())
}
