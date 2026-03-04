use anyhow::Result;

use crate::tmux;

pub fn run() -> Result<()> {
    if !tmux::session_exists() {
        println!("No kiln session found.");
        return Ok(());
    }

    println!("Stopping kiln session...");
    tmux::kill_session()?;
    println!("kiln session terminated.");
    Ok(())
}
