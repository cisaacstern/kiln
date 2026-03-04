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

/// Create the kiln tmux session with 4-pane layout
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

    // Split pane 0 vertically to create bottom area (pane 1)
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

    // Split pane 1 vertically to create pane 2 (kiln status, bottom-left lower)
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

    // Split pane 1 horizontally to create pane 3 (claude code, bottom-right)
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

/// Run a tmux command and capture stdout
fn run_tmux_output(args: &[&str]) -> Result<String> {
    let output = Command::new("tmux")
        .args(args)
        .output()
        .context("Failed to run tmux")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("tmux command failed: {}", stderr);
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Set a pane's title
pub fn set_pane_title(pane_id: &str, title: &str) -> Result<()> {
    run_tmux(&["select-pane", "-t", pane_id, "-T", title])
}

#[derive(Debug, Clone)]
pub struct PaneInfo {
    pub pane_id: String,
    pub title: String,
    pub window_index: String,
}

/// List all panes in the kiln session
pub fn list_session_panes() -> Result<Vec<PaneInfo>> {
    let output = run_tmux_output(&[
        "list-panes",
        "-s",
        "-t",
        SESSION_NAME,
        "-F",
        "#{pane_id}\t#{pane_title}\t#{window_index}",
    ])?;
    let mut panes = Vec::new();
    for line in output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 3 {
            panes.push(PaneInfo {
                pane_id: parts[0].to_string(),
                title: parts[1].to_string(),
                window_index: parts[2].to_string(),
            });
        }
    }
    Ok(panes)
}

/// Swap two panes
pub fn swap_pane(src_id: &str, dst_id: &str) -> Result<()> {
    run_tmux(&["swap-pane", "-s", src_id, "-t", dst_id])
}

/// Find the active Claude pane in window 0 by title (starts with "claude-")
pub fn active_claude_pane_id() -> Result<String> {
    let panes = list_session_panes()?;
    panes
        .iter()
        .find(|p| p.window_index == "0" && p.title.starts_with("claude-"))
        .map(|p| p.pane_id.clone())
        .context("No active Claude pane in window 0")
}

/// Find the parking window index, if it exists
fn find_parking_window() -> Result<Option<String>> {
    let output = run_tmux_output(&[
        "list-windows",
        "-t",
        SESSION_NAME,
        "-F",
        "#{window_index}\t#{window_name}",
    ])?;
    for line in output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() >= 2 && parts[1] == "park" {
            return Ok(Some(parts[0].to_string()));
        }
    }
    Ok(None)
}

/// Create a new Claude pane in the parking window, return its pane ID
pub fn create_parked_pane(dir: &str) -> Result<String> {
    let pane_id = if let Some(win_idx) = find_parking_window()? {
        // Split inside existing parking window
        run_tmux_output(&[
            "split-window",
            "-t",
            &format!("{SESSION_NAME}:{win_idx}"),
            "-d",
            "-c",
            dir,
            "-P",
            "-F",
            "#{pane_id}",
        ])?
    } else {
        // Create new parking window
        run_tmux_output(&[
            "new-window",
            "-d",
            "-t",
            &format!("{SESSION_NAME}:"),
            "-n",
            "park",
            "-c",
            dir,
            "-P",
            "-F",
            "#{pane_id}",
        ])?
    };
    // Start claude in the new pane
    run_tmux(&["send-keys", "-t", &pane_id, "claude", "Enter"])?;
    Ok(pane_id)
}

/// Select/focus a window by target
pub fn select_window(target: &str) -> Result<()> {
    run_tmux(&["select-window", "-t", target])
}

/// Get the pane ID for a target specifier (e.g. "kiln:0.3")
pub fn get_pane_id(target: &str) -> Result<String> {
    run_tmux_output(&["display-message", "-t", target, "-p", "#{pane_id}"])
}

/// Get the title for a pane by its pane ID (e.g. "%5")
pub fn get_pane_title(pane_id: &str) -> Result<String> {
    run_tmux_output(&["display-message", "-t", pane_id, "-p", "#{pane_title}"])
}

/// Split a pane horizontally, returning the new pane ID.
/// If `before` is true, the new pane is created to the left.
/// `size` is the width in columns for the new pane.
pub fn split_pane_horizontal(target: &str, size: u32, dir: &str, before: bool) -> Result<String> {
    let size_str = size.to_string();
    let mut args = vec![
        "split-window",
        "-t",
        target,
        "-h",
        "-l",
        &size_str,
        "-c",
        dir,
        "-P",
        "-F",
        "#{pane_id}",
    ];
    if before {
        args.push("-b");
    }
    run_tmux_output(&args)
}

/// Send keys to a specific pane by pane ID.
/// Safety: `keys` must never contain user-controlled input.
pub(crate) fn send_keys_to_pane(pane_id: &str, keys: &str) -> Result<()> {
    run_tmux(&["send-keys", "-t", pane_id, keys, "Enter"])
}
