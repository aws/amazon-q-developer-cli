# Figterm

Figterm is how hook into the user's system to get information about the user's
current shell session like:

- What the user has typed
- Environment variables
- Working directory

It is run in each terminal session started by the user, and communicates this
information to the mac app, sending update notifications on edit buffer changes,
prompts (precmd), and right before a command is executed (preexec)

## Installation/Usage

First, make sure shell integrations are installed. You can use the `q_cli` to do
this:

```
q integrations install dotfiles
```

The run `make install` to build the binary and move to the right location.

The shell integrations will then launch figterm on each terminal session.

You can verify figterm is running by:

1. Running `pstree -p $$` and checking, e.g. for a `figterm` process with a
   child `zsh` process.
2. Running `env | grep FIG` and checking the `Q_TERM` variable is set to
3.

## How does it work?

When you spin up a terminal emulator like iTerm, it launches a shell. If our
[shell integrations](../fig_integrations/src/shell/) are properly installed
into a user's dotfiles (.zshrc, .bashrc, etc.) then the shell will source the
`pre` integration to [exec](../fig_integrations/src/shell/scripts/pre.sh)
the figterm binary.

Figterm launches a PTY, or pseudoterminal, (see
https://spin0r.wordpress.com/2012/12/28/terminally-confused-part-seven/ for an
explanation of PTYs). In short, it forks a child process that execs a shell. The
parent process, then, is responsible for acting as a "pseudoterminal", which is
roughly a process that a shell reads from and writes to. In figterm, this parent
process:

1. Acts a pass through layer between shell and terminal emulator
2. Creates a representation of the terminal screen state (it's a headless
   terminal emulator)
3. Communicates events with shell context to the macOS app.

**Figterm as a Shell & Terminal Intermediary**

Without figterm, iTerm and zsh communicate directly: `iTerm <-> zsh`. iTerm will
forward input from the user to `zsh` and `zsh` will send ANSI escape codes to
the terminal iTerm to direct it to move the cursor, change text color, write the
prompt, write command output to the screen, etc.

With figterm, we intercept the shell/terminal emulator communication:
`iTerm <-> figterm <-> zsh`. The figterm process looks like a shell to iTerm (it
forwards ANSI escape codes from `zsh` to iTerm), and looks like a terminal to
`zsh` (it forwards input to `zsh`).

**Figterm as a Headless Terminal**

A terminal emulator like iTerm has an internal representation of what is
displayed to the terminal screen. This is usually stored as a grid of "cells"
that each have attributes like foreground color, background color, character,
etc. The terminal processes ANSI escape sequences from the shell to update this
representation.

Figterm replicates the processing of these sequences from the shell to create
it's own internal screen representation. We do this with a fork of
[Alacritty](https://github.com/alacritty/alacritty)’s
[alacritty_terminal crate](https://docs.rs/alacritty_terminal/latest/alacritty_terminal/index.html).
This lives in [`crates/alacritty_terminal/`](../alacritty_terminal/).

Our `post` [shell integrations](../fig_integrations/src/shell/scripts/) add
hooks that print custom OSC ANSI escape codes that `figterm` also parses. This
is our mechanism for sending information from the shell to `figterm`. We send
these codes to `figterm` to:

1. Indicate the start/end of a prompt
2. Indicate a command is about to run
3. Update context about the shell (env variables, working directory, etc.)

Figterm then uses these internally to annotate screen cells based on whether
they are part of a shell prompt, the "edit buffer" that the user has typed, or
command output.

**Figterm as a Shell Context Provider**

Figterm sends 3 types of hooks with shell context to the macOS app:

- `prompt` or `precmd` events - sent right before a prompt is displayed
- `preexec` events - sent right before a command is executed
- `editBuffer` events - sent whenever the edit buffer is updated

Figterm computes the current edit buffer from its semantically annotated screen
representation on any screen updates.

Each of these events also contains the most recent context of environment
variables, working directory, etc.
