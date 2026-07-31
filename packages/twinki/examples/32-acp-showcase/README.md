# Twinki ACP Showcase

A compact, single-session ACP client that demonstrates Twinki's application
surface without reproducing Muxi's production architecture.

## Run

From this directory:

```bash
bun run example:acp-showcase -- --cwd .
```

The default process is:

```text
kiro-cli acp --agent-engine v3
```

Run against another workspace or ACP agent:

```bash
bun run example:acp-showcase -- --cwd /path/to/project

bun run example:acp-showcase -- \
  --command my-agent \
  --agent-arg serve \
  --cwd /path/to/project
```

The default theme is `kiro-dark`. Available themes are `kiro-dark`, `graphite`,
`paper`, `contrast`, `monokai`, `dracula`, `github-dark`, `catppuccin`, `nord`,
`one-dark`, and `tokyo-night`:

```bash
bun run example:acp-showcase -- --theme tokyo-night --cwd .
```

## Interaction

- Click folders to expand the workspace tree and files to open their read-only,
  syntax-highlighted preview.
- Right-click a file for preview, prompt, agent, and theme actions.
- Use the mouse wheel over the rail, transcript, or file to scroll that region.
- Drag terminal text to copy the scoped selection. OSC 52 is used everywhere,
  with a local `pbcopy` fallback on macOS; the status line confirms the copy.
- Click tabs and scrollbars directly.
- Click the theme name in the status line or press `Ctrl+G` to cycle themes.
- Click `? HELP` or press `F1` / `Ctrl+/` to open the shortcut help dialog.
- Press `Ctrl+Tab` to switch Agent/File views.
- Press `Ctrl+S` to toggle `STEER` and `QUEUE` while using KAS V3.
- Submit during a turn to enqueue a local FIFO follow-up by default. In `STEER`
  mode, native steering failures fall back to that queue.
- Press `Escape` or `Ctrl+X` to cancel an active turn.
- Overlay panels support arrows, Enter, number selection, and Escape.

All filesystem reads are contained under `--cwd`. The rail is capped at 300
files, and previews reject binary files and content larger than 400 KB.
