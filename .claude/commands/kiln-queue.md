Get the current tmux pane title by running: `tmux display-message -p '#{pane_title}'`

The title will be in the format `claude-<plan-name>`. Extract the plan name (everything after `claude-`).

Then run: `kiln queue <plan-name>`

If the pane title is `claude-main`, tell the user this is the default session with no plan associated.
