# 30-overlay-panel

A standalone demo of twinki's new panel primitives: **Tabs**, **Split**, and overlay positioning — composed into a tabbed note viewer with a command-palette overlay.

## Run

```bash
cd packages/twinki && npx tsx examples/30-overlay-panel/index.tsx
```

## What it demonstrates

| Primitive | Usage in this example |
|-----------|----------------------|
| `Tabs` + `useTabs` | Tab strip with Ctrl+1..9 jump, Ctrl+Tab cycle, Ctrl+w close |
| `Split` | Sidebar (note list) + content pane, ← → resizable |
| `Box position="absolute"` | Command-palette overlay centered on screen |
| `useInput` | Full keyboard handling (Kitty protocol) |

## Keys

| Key | Action |
|-----|--------|
| Ctrl+p | Toggle command palette |
| Ctrl+Tab / Ctrl+Shift+Tab | Cycle tabs forward / back |
| Ctrl+1..9 | Jump to tab by index |
| Ctrl+w | Close active tab |
| ← → | Resize split ratio |
| q | Quit |

## Architecture

```
App
├── Tabs (strip)
├── Split (row)
│   ├── Pane A: note list sidebar
│   └── Pane B: active note content
├── Footer (hints)
└── Palette (absolute overlay, shown on Ctrl+p)
```

This is the minimal pattern for a panel-based TUI app. Copy it as a starting point for more complex layouts (the full multiplexer in `examples/29-multiplexer` extends this pattern with sessions, ACP backends, and 95 scenarios).
