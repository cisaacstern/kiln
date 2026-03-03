use anyhow::{Result, bail};
use std::io::Write;
use std::process::Command;

use crate::state::{self, Status};
use crate::tsk;

pub fn run(name: &str) -> Result<()> {
    let state_map = state::read_state()?;
    let task = state::get_task(&state_map, name)?;

    if task.status != Status::PendingReview {
        bail!(
            "Task '{}' is in state {}, expected PENDING_REVIEW",
            name,
            task.status
        );
    }

    // Validate branch/ref names before passing to external commands
    state::validate_git_ref(&task.tsk_branch)?;
    state::validate_git_ref(&task.base_ref)?;

    // Transition to IN_REVIEW
    state::update_task_status(name, Status::InReview)?;
    println!("Starting review for '{name}'...");

    // Launch difit
    let output = Command::new("npx")
        .args(["difit", &task.tsk_branch, &task.base_ref])
        .output();

    let output = match output {
        Ok(o) => o,
        Err(e) => {
            // Revert state on failure to launch
            state::update_task_status(name, Status::PendingReview)?;
            bail!("Failed to launch difit: {}", e);
        }
    };

    let stdout = String::from_utf8_lossy(&output.stdout);
    let comments = extract_comments(&stdout);

    match comments {
        Some(comments) if !comments.is_empty() => {
            println!("Review comments captured. Queuing follow-up...");
            queue_followup(name, &task, &comments)?;
        }
        _ => {
            // No comments - keep as PENDING_REVIEW so user can approve
            state::update_task_status(name, Status::PendingReview)?;
            println!("No review comments. Task remains PENDING_REVIEW.");
            println!("Use 'kiln approve {name}' to approve, or 'kiln review {name}' to review again.");
        }
    }

    Ok(())
}

/// Extract comments from difit output.
/// Comments appear between `===...===` separator lines.
fn extract_comments(output: &str) -> Option<String> {
    let separator = "==================================================";
    let sections: Vec<&str> = output.split(separator).collect();
    sections
        .get(1)
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Queue a follow-up task with the review comments as the new prompt
fn queue_followup(name: &str, task: &state::TaskState, comments: &str) -> Result<()> {
    // Validate task name before using it in file paths
    state::validate_task_name(name)?;

    // Write comments to a temp file as the new prompt
    let kiln_dir = state::kiln_dir()?;
    let prompt_path = kiln_dir.join(format!("{name}-review-{}.md", task.revision + 1));
    let mut file = std::fs::File::create(&prompt_path)?;

    writeln!(file, "# Review feedback (revision {})", task.revision + 1)?;
    writeln!(file)?;
    writeln!(file, "The following review comments were provided for the previous implementation.")?;
    writeln!(file, "Please address all feedback:")?;
    writeln!(file)?;
    writeln!(file, "{comments}")?;

    // Transition to CHANGES_REQUESTED before re-queuing
    state::update_task_status(name, Status::ChangesRequested)?;

    // Queue new tsk task
    let (tsk_id, tsk_branch) = tsk::add_task(name, &prompt_path)?;

    // Update state with new tsk task info
    let mut state_map = state::read_state()?;
    if let Some(t) = state_map.get_mut(name) {
        t.status = Status::Queued;
        t.tsk_id = tsk_id;
        t.tsk_branch = tsk_branch;
        t.revision += 1;
        t.updated_at = chrono::Utc::now();
    }
    state::write_state(&state_map)?;

    println!("Follow-up queued (revision {})", task.revision + 1);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_comments_with_content() {
        let output = "Starting server...\n\
                       ==================================================\n\
                       file.rs:10 - This should use a match instead\n\
                       file.rs:25 - Missing error handling\n\
                       ==================================================\n\
                       Server stopped.";
        let comments = extract_comments(output).unwrap();
        assert!(comments.contains("This should use a match"));
        assert!(comments.contains("Missing error handling"));
    }

    #[test]
    fn test_extract_comments_empty() {
        let output = "Starting server...\n\
                       ==================================================\n\
                       ==================================================\n\
                       Server stopped.";
        let comments = extract_comments(output);
        assert!(comments.is_none());
    }

    #[test]
    fn test_extract_comments_no_separator() {
        let output = "Starting server...\nServer stopped.";
        let comments = extract_comments(output);
        assert!(comments.is_none());
    }
}
