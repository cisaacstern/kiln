mod commands;
mod config;
mod state;
mod tmux;
mod tsk;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "kiln", about = "Agentic development workflow CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create tmux session with 5-pane layout, start tsk server, status watcher
    Start,
    /// Queue a task via tsk add with state tracking
    Queue {
        /// Task name
        name: String,
        /// Path to prompt file
        #[arg(long)]
        prompt_file: PathBuf,
    },
    /// Launch difit for diff review, capture comments, auto-queue follow-up
    Review {
        /// Task name to review
        name: String,
    },
    /// Mark task approved, print merge/push instructions
    Approve {
        /// Task name to approve
        name: String,
    },
    /// Show combined tsk + review lifecycle status
    Status {
        /// Continuously watch status (3-second refresh)
        #[arg(long)]
        watch: bool,
        /// Internal: run as status watcher daemon (syncs tsk state)
        #[arg(long, hide = true)]
        daemon: bool,
    },
    /// Tear down tmux session and tsk server
    Stop,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start => commands::start::run(),
        Commands::Queue { name, prompt_file } => commands::queue::run(&name, &prompt_file),
        Commands::Review { name } => commands::review::run(&name),
        Commands::Approve { name } => commands::approve::run(&name),
        Commands::Status { watch, daemon } => commands::status::run(watch, daemon),
        Commands::Stop => commands::stop::run(),
    }
}
