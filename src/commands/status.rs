use anyhow::Result;
use colored::Colorize;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use crate::state::{self, Status};
use crate::tmux;
use crate::tsk;

pub fn run(watch: bool, daemon: bool) -> Result<()> {
    if watch || daemon {
        let running = Arc::new(AtomicBool::new(true));
        let r = running.clone();
        ctrlc::set_handler(move || {
            r.store(false, Ordering::SeqCst);
        })?;

        while running.load(Ordering::SeqCst) {
            if daemon {
                sync_tsk_state()?;
                update_window_name()?;
            }
            clear_screen();
            print_status()?;
            thread::sleep(Duration::from_secs(3));
        }
    } else {
        print_status()?;
    }
    Ok(())
}

fn clear_screen() {
    // ANSI clear screen + move cursor to top-left
    print!("\x1B[2J\x1B[H");
}

fn print_status() -> Result<()> {
    let state = state::read_state()?;

    println!("{}", "═══ kiln status ═══".bold());
    println!();

    if state.is_empty() {
        println!("  No tasks tracked yet. Use 'kiln queue' to add a task.");
        return Ok(());
    }

    // Sort tasks by updated_at for consistent display
    let mut tasks: Vec<_> = state.iter().collect();
    tasks.sort_by(|a, b| a.1.updated_at.cmp(&b.1.updated_at));

    // Print header
    println!(
        "  {:<20} {:<20} {:<6} {:<12} {}",
        "NAME".bold(),
        "STATUS".bold(),
        "REV".bold(),
        "TSK ID".bold(),
        "UPDATED".bold(),
    );
    println!("  {}", "─".repeat(75));

    for (name, task) in &tasks {
        let status_str = format_status(task.status);
        let updated = task.updated_at.format("%H:%M:%S");
        println!(
            "  {:<20} {:<20} {:<6} {:<12} {}",
            name, status_str, task.revision, task.tsk_id, updated
        );
    }

    println!();

    // Summary line
    let pending = state::count_by_status(&state, Status::PendingReview);
    let running = state::count_by_status(&state, Status::Running);
    let changes = state::count_by_status(&state, Status::ChangesRequested);

    let mut summary_parts = Vec::new();
    if running > 0 {
        summary_parts.push(format!("{running} running"));
    }
    if pending > 0 {
        summary_parts.push(format!("{pending} pending review"));
    }
    if changes > 0 {
        summary_parts.push(format!("{changes} changes requested"));
    }

    if !summary_parts.is_empty() {
        println!("  {}", summary_parts.join(" | "));
    }

    Ok(())
}

fn format_status(status: Status) -> String {
    match status {
        Status::Queued => "QUEUED".dimmed().to_string(),
        Status::Running => "RUNNING".blue().to_string(),
        Status::PendingReview => "PENDING_REVIEW".yellow().to_string(),
        Status::InReview => "IN_REVIEW".cyan().to_string(),
        Status::ChangesRequested => "CHANGES_REQ".magenta().to_string(),
        Status::Approved => "APPROVED".green().to_string(),
        Status::Failed => "FAILED".red().to_string(),
    }
}

/// Sync tsk task statuses into kiln state
fn sync_tsk_state() -> Result<()> {
    let tsk_tasks = match tsk::list_tasks() {
        Ok(tasks) => tasks,
        Err(_) => return Ok(()), // tsk not available, skip sync
    };

    let mut state = state::read_state()?;
    let mut changed = false;

    for (name, task) in state.iter_mut() {
        // Find matching tsk task
        let tsk_task = tsk_tasks.iter().find(|t| t.name == *name || t.id == task.tsk_id);

        if let Some(tsk_task) = tsk_task {
            let tsk_status = tsk_task.status.to_uppercase();
            match task.status {
                Status::Queued if tsk_status == "RUNNING" || tsk_status == "IN_PROGRESS" => {
                    task.status = Status::Running;
                    task.updated_at = chrono::Utc::now();
                    changed = true;
                }
                Status::Queued | Status::Running if tsk_status == "COMPLETE" || tsk_status == "DONE" => {
                    task.status = Status::PendingReview;
                    task.updated_at = chrono::Utc::now();
                    changed = true;
                }
                Status::Queued | Status::Running if tsk_status == "FAILED" || tsk_status == "ERROR" => {
                    task.status = Status::Failed;
                    task.updated_at = chrono::Utc::now();
                    changed = true;
                }
                _ => {}
            }

            // Update branch if tsk reports one
            if !tsk_task.branch.is_empty() && tsk_task.branch != task.tsk_branch {
                task.tsk_branch = tsk_task.branch.clone();
                changed = true;
            }
        }
    }

    if changed {
        state::write_state(&state)?;
    }

    Ok(())
}

fn update_window_name() -> Result<()> {
    let state = state::read_state()?;
    let pending = state::count_by_status(&state, Status::PendingReview);
    let changes = state::count_by_status(&state, Status::ChangesRequested);
    let name = tmux::build_window_name(pending, changes);
    tmux::set_window_name(&name)?;
    Ok(())
}
