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
    let repos = state::list_all_repos()?;

    println!("{}", "═══ kiln status ═══".bold());
    println!();

    // Collect all tasks across repos with repo name attached
    let mut all_tasks: Vec<(String, String, state::TaskState)> = Vec::new();
    for (repo_name, repo_path) in &repos {
        let repo_state = state::read_state_for_dir(repo_path)?;
        for (task_name, task) in repo_state {
            all_tasks.push((repo_name.clone(), task_name, task));
        }
    }

    if all_tasks.is_empty() {
        println!("  No tasks tracked yet. Use 'kiln queue' to add a task.");
        return Ok(());
    }

    // Sort by repo name, then by updated_at
    all_tasks.sort_by(|a, b| a.0.cmp(&b.0).then(a.2.updated_at.cmp(&b.2.updated_at)));

    // Print header
    println!(
        "  {:<12} {:<20} {:<20} {:<6} {:<12} {}",
        "REPO".bold(),
        "NAME".bold(),
        "STATUS".bold(),
        "REV".bold(),
        "TSK ID".bold(),
        "UPDATED".bold(),
    );
    println!("  {}", "─".repeat(85));

    for (repo, name, task) in &all_tasks {
        let status_str = format_status(task.status);
        let updated = task.updated_at.format("%H:%M:%S");
        println!(
            "  {:<12} {:<20} {:<20} {:<6} {:<12} {}",
            repo, name, status_str, task.revision, task.tsk_id, updated
        );
    }

    println!();

    // Summary line — aggregate across all repos
    let running = all_tasks.iter().filter(|t| t.2.status == Status::Running).count();
    let pending = all_tasks.iter().filter(|t| t.2.status == Status::PendingReview).count();
    let changes = all_tasks.iter().filter(|t| t.2.status == Status::ChangesRequested).count();

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

/// Sync tsk task statuses into kiln state across all repos
fn sync_tsk_state() -> Result<()> {
    let tsk_tasks = match tsk::list_tasks() {
        Ok(tasks) => tasks,
        Err(_) => return Ok(()), // tsk not available, skip sync
    };

    let repos = state::list_all_repos()?;

    for (_repo_name, repo_path) in &repos {
        let mut state = state::read_state_for_dir(repo_path)?;
        if state.is_empty() {
            continue;
        }
        let mut changed = false;

        for (name, task) in state.iter_mut() {
            // Find matching tsk task
            let tsk_task = tsk_tasks.iter()
                .find(|t| t.id == task.tsk_id)
                .or_else(|| tsk_tasks.iter().find(|t| t.name == *name));

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
            state::write_state_to_dir(repo_path, &state)?;
        }
    }

    Ok(())
}

fn update_window_name() -> Result<()> {
    let repos = state::list_all_repos()?;
    let mut pending = 0;
    let mut changes = 0;
    for (_repo_name, repo_path) in &repos {
        let repo_state = state::read_state_for_dir(repo_path)?;
        pending += state::count_by_status(&repo_state, Status::PendingReview);
        changes += state::count_by_status(&repo_state, Status::ChangesRequested);
    }
    let name = tmux::build_window_name(pending, changes);
    tmux::set_window_name(&name)?;
    Ok(())
}
