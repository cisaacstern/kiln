use anyhow::{Context, Result};
use std::io::{self, Read, Write};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::state::{self, PaneStatus};
use crate::tmux;

/// Run the sidebar daemon that displays Claude pane statuses and handles input.
pub fn run_daemon() -> Result<()> {
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .context("Failed to set Ctrl+C handler")?;

    // Enable raw mode for non-blocking keyboard input
    enable_raw_mode()?;
    let _guard = RawModeGuard;

    // Hide cursor
    print!("\x1b[?25l");
    let _ = io::stdout().flush();

    while running.load(Ordering::SeqCst) {
        render()?;

        // Non-blocking stdin read with ~1s timeout (poll in 100ms chunks)
        for _ in 0..10 {
            if !running.load(Ordering::SeqCst) {
                break;
            }
            if let Some(key) = read_key_nonblocking()? {
                handle_key(key)?;
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    // Show cursor on exit
    print!("\x1b[?25h");
    let _ = io::stdout().flush();

    Ok(())
}

fn render() -> Result<()> {
    let panes = claude_panes_sorted()?;
    let statuses = state::read_pane_status().unwrap_or_default();
    let active_idx = panes.iter().position(|p| p.window_index == "0");

    // Move cursor to top-left and clear screen
    print!("\x1b[H\x1b[2J");

    println!("\x1b[1m═══ sessions ═══\x1b[0m");
    println!();

    for (i, pane) in panes.iter().enumerate() {
        let status = statuses
            .get(&pane.title)
            .map(|e| e.status)
            .unwrap_or(PaneStatus::Unknown);
        let icon = status.icon();
        let marker = if Some(i) == active_idx { " *" } else { "" };
        println!(" {} {}{}", icon, pane.title, marker);
    }

    if panes.is_empty() {
        println!(" (no sessions)");
    }

    println!();
    println!(" \x1b[2m[n]ext [p]rev\x1b[0m");

    let _ = io::stdout().flush();
    Ok(())
}

fn handle_key(key: u8) -> Result<()> {
    match key {
        b'n' => {
            let _ = switch_pane(Direction::Next);
        }
        b'p' => {
            let _ = switch_pane(Direction::Prev);
        }
        b'0'..=b'9' => {
            let idx = (key - b'0') as usize;
            let _ = switch_to_index(idx);
        }
        _ => {}
    }
    Ok(())
}

enum Direction {
    Next,
    Prev,
}

fn switch_pane(dir: Direction) -> Result<()> {
    let panes = claude_panes_sorted()?;
    if panes.len() < 2 {
        return Ok(());
    }

    let current = panes
        .iter()
        .position(|p| p.window_index == "0")
        .context("No active Claude pane")?;

    let target = match dir {
        Direction::Next => (current + 1) % panes.len(),
        Direction::Prev => {
            if current == 0 {
                panes.len() - 1
            } else {
                current - 1
            }
        }
    };

    let active_id = tmux::active_claude_pane_id()?;
    tmux::swap_pane(&panes[target].pane_id, &active_id)?;
    Ok(())
}

fn switch_to_index(idx: usize) -> Result<()> {
    let panes = claude_panes_sorted()?;
    if idx >= panes.len() {
        return Ok(());
    }

    // Skip if already active
    if panes[idx].window_index == "0" {
        return Ok(());
    }

    let active_id = tmux::active_claude_pane_id()?;
    tmux::swap_pane(&panes[idx].pane_id, &active_id)?;
    Ok(())
}

fn claude_panes_sorted() -> Result<Vec<tmux::PaneInfo>> {
    let all = tmux::list_session_panes()?;
    let mut claude: Vec<_> = all
        .into_iter()
        .filter(|p| p.title.starts_with("claude-"))
        .collect();
    claude.sort_by(|a, b| a.title.cmp(&b.title));
    Ok(claude)
}

// --- Raw terminal mode using libc ---

fn enable_raw_mode() -> Result<()> {
    #[cfg(unix)]
    unsafe {
        let mut termios: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(libc::STDIN_FILENO, &mut termios) != 0 {
            anyhow::bail!("tcgetattr failed");
        }
        // Disable canonical mode and echo
        termios.c_lflag &= !(libc::ICANON | libc::ECHO);
        // Set minimum chars to 0, timeout to 0 (non-blocking)
        termios.c_cc[libc::VMIN] = 0;
        termios.c_cc[libc::VTIME] = 0;
        if libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &termios) != 0 {
            anyhow::bail!("tcsetattr failed");
        }
    }
    Ok(())
}

fn disable_raw_mode() {
    #[cfg(unix)]
    unsafe {
        let mut termios: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(libc::STDIN_FILENO, &mut termios) == 0 {
            termios.c_lflag |= libc::ICANON | libc::ECHO;
            termios.c_cc[libc::VMIN] = 1;
            termios.c_cc[libc::VTIME] = 0;
            let _ = libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &termios);
        }
    }
}

struct RawModeGuard;

impl Drop for RawModeGuard {
    fn drop(&mut self) {
        disable_raw_mode();
    }
}

fn read_key_nonblocking() -> Result<Option<u8>> {
    let mut buf = [0u8; 1];
    let stdin = io::stdin();
    let mut handle = stdin.lock();
    match handle.read(&mut buf) {
        Ok(1) => Ok(Some(buf[0])),
        Ok(_) => Ok(None),
        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => Ok(None),
        Err(e) => Err(e.into()),
    }
}
