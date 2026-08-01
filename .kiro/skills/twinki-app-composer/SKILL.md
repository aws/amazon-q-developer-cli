---
name: twinki-app-composer
description: Compose Twinki terminal applications from constrained JSON using registered React widgets. Use when a user asks to create, rearrange, simplify, or explain a Kiro Composer layout, or explicitly requests a custom React widget for the composer.
---

# Twinki App Composer

Use the mode, request, paths, and registered widgets supplied here:

$ARGUMENTS

When runtime arguments say `Invocation: ambient composer chat`, first classify the request:

- For composer requests, including natural-language requests to add, remove, move, split, or configure UI, follow the Runtime Composer workflow.
- For ordinary chat or coding requests unrelated to the composer UI, answer normally and do not edit layout files.

An explicit `/layout` invocation is always a composition request. If a composition request is incomplete, inspect the active layout and ask which workflow or widgets the user wants before writing.

## Runtime Composer

Use this mode when the arguments say `Mode: runtime composer`.

1. Use the injected parsed layout and widget capabilities to understand the current screen, then read the active layout and `layout.ts` before editing.
2. Prefer changing or creating one JSON file in the supplied layouts directory. Do not edit the renderer for a JSON-only request.
3. Use only registered widget ids supplied in the arguments.
4. Preserve valid JSON during targeted edits so the running CLI can hot-reload it.
5. Validate with `bun <composer-entry> --layout <layout-path> --check`.

Runtime files have this root:

```json
{
  "version": 1,
  "title": "Developer",
  "layout": { "type": "widget", "widget": "chat" }
}
```

- A `widget` node has `type`, `widget`, optional unique `id`, and optional `title`. Its id defaults to the widget name.
- A `split` node has unique `id`, `direction` (`row` or `column`), `ratio` from `0.15` through `0.85`, and exactly two `children`.
- A `tabs` node has unique `id`, `defaultTab`, and non-empty `tabs`. Each tab has unique `id`, `title`, and `child`.
- `chat` and `editor` nodes define canvas slots. Files, diffs, and sessions can share tabs in either slot; two slots allow left/right canvas splitting.
- Sessions are runtime state, not layout nodes. Use the `session` widget to create, rename, close, and focus them. Each session owns one ACP connection and can be opened in either canvas slot.
- Use `settings` for theme and Ask/YOLO permission mode.
- Keep nesting shallow and make chat usable at 80x24.
- Create a new descriptively named JSON file when asked for a new layout. Update the active file only when asked to modify the current layout.

For an explicitly requested new React widget, read `widgets.tsx` and the registry in `App.tsx`. Implement it with `defineWidget`, consume only the `WidgetFrame` API, register it once, and then reference its id from JSON. Explain that TSX changes require restarting the sample; JSON changes hot-reload.

## Safety

Never place commands, executable paths, imports, event handlers, or arbitrary expressions in layout JSON. Keep layouts declarative and within the selected mode's closed schema.
