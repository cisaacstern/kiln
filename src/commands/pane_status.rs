use anyhow::{Context, Result, bail};
use chrono::Utc;

use crate::state::{self, PaneStatus, PaneStatusEntry};
use crate::tmux;

/// Set the pane status for the calling Claude pane.
/// Identifies the pane via $TMUX_PANE env var, looks up its title,
/// and writes the status to the kiln state directory.
pub fn run(status_str: &str) -> Result<()> {
    let status: PaneStatus = status_str.parse()?;

    let pane_id = std::env::var("TMUX_PANE")
        .context("TMUX_PANE not set — this command must be run inside a tmux pane")?;

    let title = tmux::get_pane_title(&pane_id)
        .context("Failed to get pane title from tmux")?;

    if !title.starts_with("claude-") {
        bail!("Current pane '{}' is not a Claude pane (title: {})", pane_id, title);
    }

    let mut map = state::read_pane_status()?;
    map.insert(
        title,
        PaneStatusEntry {
            status,
            updated_at: Utc::now(),
        },
    );
    state::write_pane_status(&map)?;

    Ok(())
}
