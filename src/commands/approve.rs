use anyhow::{Result, bail};

use crate::state::{self, Status};

pub fn run(name: &str) -> Result<()> {
    let state_map = state::read_state()?;
    let task = state::get_task(&state_map, name)?;

    match task.status {
        Status::PendingReview | Status::InReview => {}
        _ => {
            bail!(
                "Task '{}' is in state {}, expected PENDING_REVIEW or IN_REVIEW",
                name,
                task.status
            );
        }
    }

    state::update_task_status(name, Status::Approved)?;

    println!("Task '{name}' approved!");
    println!();
    println!("Branch: {}", task.tsk_branch);
    println!("Base:   {} ({})", task.base_ref, &task.base_commit[..8]);
    println!();
    println!("To merge:");
    println!("  git checkout {}", task.base_ref);
    println!("  git merge {}", task.tsk_branch);
    println!();
    println!("Or push the branch for PR:");
    println!("  git push origin {}", task.tsk_branch);

    Ok(())
}
