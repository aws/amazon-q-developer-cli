# Kiro Composer

A chat-first composable CLI built from Twinki primitives. It starts as bare Kiro chat, then composes registered React widgets from hot-reloaded JSON layouts.

## Run

From `packages/twinki`:

```bash
bun run example:kiro-composer -- --cwd .
```

Use `--engine v2` or `--engine v3` to select the Kiro ACP engine. The default is v3.

## Compose

Press `Ctrl+L` to choose Chat, Developer, Full, Multi-session, Review, generated layouts, or **Create with Kiro**. You can also ask naturally to add, remove, or rearrange the interface; the composer skill is injected into every turn and stays dormant for unrelated chat. The create action puts the explicit `/layout ` shortcut in chat. Describe the workspace you want and approve the JSON write. Layouts use only three node types:

```json
{
  "type": "split",
  "id": "main",
  "direction": "row",
  "ratio": 0.6,
  "children": [
    { "type": "widget", "widget": "editor" },
    { "type": "widget", "widget": "chat" }
  ]
}
```

Registered widgets are `chat`, `files`, `git`, `session`, `settings`, and `editor`. Widget `id` identifies an instance and defaults to its widget name.

`chat` and `editor` are canvas slots. Files, diffs, and ACP sessions open as tabs in the active canvas slot, so the same group can move from a conversation to source or a diff without changing layouts. Right-click a canvas tab to split it into the left or right group, close the view, or manage its session.

Sessions are independent of canvas slots. The Session widget creates, renames, closes, and focuses them; focusing opens the session as a canvas tab. Each session lazily owns one ACP connection and keeps its transcript while its tab moves between groups.

The Settings widget cycles themes and switches permissions between **Ask** and **YOLO**. Ask opens the normal permission dialog. YOLO automatically selects an allow option when one is available.

Developers can add React widgets with `defineWidget`, add the type to `BUILTIN_WIDGET_IDS`, and register it in `App.tsx`; JSON remains inert configuration.

## Controls

- `Ctrl+1`, `Ctrl+2`, `Ctrl+3`, `Ctrl+4`: Files, Git, Sessions, and Settings
- `Ctrl+Tab`: cycle tabs in the active canvas group
- `Ctrl+W`: close the active canvas tab
- `Ctrl+L`: open the layout picker
- `Ctrl+R`: refresh workspace and Git state
- `Ctrl+G`: cycle theme
- `Ctrl+X` or `Escape`: interrupt the active turn
- `F1` or `Ctrl+/`: open keyboard help
- Right-click files, Git entries, sessions, or canvas tabs for open, split-left, split-right, close, reveal, prompt, and copy actions

Git diffs are side-by-side in wide editor panes and unified when space is tight. All reads stay under `--cwd`, Git commands use argument arrays rather than a shell, the editor is read-only, and ACP writes still require the normal permission flow.
