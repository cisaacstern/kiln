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

    let kiln_name = tmux::get_pane_option(&pane_id)
        .context("Failed to get @kiln_name from tmux")?;

    if !kiln_name.starts_with("claude-") {
        bail!("Current pane '{}' is not a Claude pane (kiln_name: {})", pane_id, kiln_name);
    }

    let mut map = state::read_pane_status()?;
    map.insert(
        kiln_name,
        PaneStatusEntry {
            status,
            updated_at: Utc::now(),
        },
    );
    state::write_pane_status(&map)?;

    Ok(())
}
