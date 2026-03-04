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
        /// Path to prompt file (optional if a plan is registered)
        #[arg(long)]
        prompt_file: Option<PathBuf>,
    },
    /// Manage Claude plan sessions
    Plan {
        #[command(subcommand)]
        action: PlanAction,
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
    /// Set pane status (called by Claude Code hooks)
    PaneStatus {
        /// Status to set: working, waiting, done, unknown
        status: String,
    },
    /// Run the sidebar info bar
    Sidebar {
        /// Run as daemon (interactive loop)
        #[arg(long)]
        daemon: bool,
    },
    /// Tear down tmux session and tsk server
    Stop,
}

#[derive(Subcommand)]
enum PlanAction {
    /// Create a new named plan with a fresh Claude pane
    New {
        /// Plan/task name
        #[arg(short, long)]
        name: String,
        /// Working directory for the Claude pane
        #[arg(long)]
        dir: Option<PathBuf>,
    },
    /// Swap next Claude pane into view
    Next,
    /// Swap previous Claude pane into view
    Prev,
    /// List all Claude panes
    List,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Start => commands::start::run(),
        Commands::Queue { name, prompt_file } => {
            commands::queue::run(&name, prompt_file.as_deref())
        }
        Commands::Plan { action } => match action {
            PlanAction::New { name, dir } => {
                commands::plan::run_new(&name, dir.as_deref())
            }
            PlanAction::Next => commands::plan::run_next(),
            PlanAction::Prev => commands::plan::run_prev(),
            PlanAction::List => commands::plan::run_list(),
        },
        Commands::Review { name } => commands::review::run(&name),
        Commands::Approve { name } => commands::approve::run(&name),
        Commands::Status { watch, daemon } => commands::status::run(watch, daemon),
        Commands::PaneStatus { status } => commands::pane_status::run(&status),
        Commands::Sidebar { daemon } => {
            if daemon {
                commands::sidebar::run_daemon()
            } else {
                println!("Use --daemon to run the sidebar in interactive mode.");
                Ok(())
            }
        }
        Commands::Stop => commands::stop::run(),
    }
}
