# kiln

An agentic development atelier.

## Design Motivation

Agentic coding involves a fragmented cycle of prompting, isolated execution, review, feedback, and re-prompting — requiring manual orchestration across multiple tools. kiln is an orchestration layer that stitches together:

- **tmux** — managed multi-pane session (main terminal, tsk server, status watcher, Claude Code panes) so everything lives in one workspace
- **tsk** — container-isolated task execution, so AI-generated code runs safely in isolation on its own branch
- **difit** — interactive diff review that captures comments; kiln automatically queues follow-up tasks from review feedback, closing the loop

The feedback loop: **queue** → **run** → **review** → feedback auto-queued → **run again** → **approve** → **merge**.

## Prerequisites

- `tmux`
- `tsk`
- `npx` (for difit)
- `git`
- `claude` (Claude Code CLI)

## Quick Start

```sh
kiln start
kiln plan new -n <name>       # create a plan session in Claude
# ... write your plan in the Claude pane ...
kiln queue <name>              # queue the task for execution
kiln review <name>             # review diffs, leave feedback
kiln approve <name>            # mark approved, get merge instructions
kiln stop
```

## Commands

| Command | Description |
|---------|-------------|
| `start` | Create tmux session, start tsk server and status watcher |
| `queue <name> [--prompt-file <path>]` | Queue a task via tsk |
| `plan new\|next\|prev\|list` | Manage Claude Code plan sessions |
| `review <name>` | Launch difit review; auto-queues follow-up on feedback |
| `approve <name>` | Mark approved, print merge instructions |
| `status [--watch]` | Show task lifecycle status |
| `stop` | Tear down tmux session and tsk server |

## Development

```sh
cargo build                    # build
cargo install --path .         # install locally
cargo run -- <command>         # run without installing
```

Rust edition 2024. Dependencies listed in `Cargo.toml`.
