# Review 18 — Input handler exclusivity

**Why this class matters.** The TUI is a layered UI: a base prompt, overlaid panels, approval dialogs, menus, and modal screens all coexist in the React tree. Each layer registers its own keypress handlers for common keys (Enter, Escape, Ctrl+C). When two handlers for the same key are active simultaneously, both fire — and the "wrong" one wins or both execute with conflicting effects. This caused approval selections to be silently rejected (#d29df4a02), Escape to dismiss both a panel and its parent (#628821997), and Enter to reject an already-approved crew tool (#fa3f6cbe6). The hard rule: **at any point in time, exactly one handler owns a given event**. Parent handlers must yield to child modal handlers, and every handler must gate on an `isActive` predicate tied to the current UI mode.

**Scope.** Any component that registers a keypress/input handler via `useKeypress`, `useInput`, `stdin.on('keypress')`, or equivalent hooks — especially when multiple such components can be mounted simultaneously. This covers approval dialogs, inline menus, command palette, voice input, agent-monitor screens, nested panels (settings, help, file browser), and the base prompt input. Concrete starting points: `useKeypress` and `useInput` call sites across `packages/tui/src/`, the `InlineLayout` component's Enter handler, approval/trust components, and any `isActive` guard pattern. New interactive overlays — for example a future diff-review modal, a multi-agent selector, or a file-picker panel — must be reviewed the same way.

## Techniques

1. **[code] Handler registration census.** Grep for all `useKeypress`, `useInput`, and `stdin.on('keypress'` registrations: `rg -n "useKeypress|useInput|stdin\.on\('keypress" packages/tui/src/`. For each key that appears in more than one handler (Enter, Escape, Ctrl+C, Ctrl+D, Ctrl+O, Tab), list every component that handles it. If two components can be mounted at the same time and both handle the same key without mutual exclusion, that is a finding.

2. **[code] isActive guard audit.** For each handler identified above, verify it is gated by an `isActive`, `isFocused`, or equivalent predicate that evaluates to `false` when a child modal/panel owns input. Grep for the guard pattern: `rg -n "isActive|isFocused|isOpen" packages/tui/src/`. A handler that fires unconditionally while a child overlay is mounted is a finding.

3. **[code] Enter key layering analysis.** Enter is the highest-conflict key — it submits prompts, confirms approvals, selects menu items, and accepts suggestions. Trace every Enter handler: `rg -n "key.*enter|name.*return" packages/tui/src/`. For each pair that can coexist (e.g., prompt submit + approval confirm), verify exactly one is disabled. If both can fire, flag it.

4. **[code] Escape key layering analysis.** Escape dismisses panels, cancels approvals, exits menus, and aborts voice input. Apply the same analysis as Enter: `rg -n "key.*escape|name.*escape" packages/tui/src/`. Verify that only the innermost active layer handles Escape — parent Escape handlers must be suppressed when a child panel is open.

5. **[code] Ctrl+C / Ctrl+D ownership.** These keys have process-level semantics (exit, EOF) but are also overloaded for cancel/abort in specific UI states. Grep: `rg -n "ctrl.*c|ctrl.*d|SIGINT" packages/tui/src/`. Verify that modal screens (agent monitor, approval) suppress the global exit handler and provide their own cancel semantics. Missing suppression caused #72f0221a6.

6. **[code] Menu/Select component exclusivity.** Ink's `<Select>` and custom menu components register their own arrow-key and Enter handlers. Verify that when a menu is active, the parent component's matching handlers are disabled. Check for `isFocused` props passed to `<Select>` or equivalent focus-management patterns.

7. **[blackbox] Approval dialog Enter test.** Open an approval dialog (trigger a tool that requires trust). Press Enter to approve. Verify the approval is accepted — not silently rejected by a competing Enter handler. Repeat with arrow-key selection before Enter. The approval callback must fire exactly once.

8. **[blackbox] Nested panel Escape test.** Open a nested panel (settings, help, or file browser) from within the main view. Press Escape. Verify only the innermost panel closes — the parent view must remain. Press Escape again to close the parent if applicable. Each press must dismiss exactly one layer.

9. **[blackbox] Rapid key-during-transition test.** Trigger a state transition (submit prompt, open panel) and immediately press Enter or Escape within 50 ms. The key must be handled by the correct post-transition handler, not the pre-transition one. Repeat 20 times; any misrouted keypress is a finding.

10. **[blackbox] Ctrl+C in modal screen.** Enter the agent-monitor screen or any modal that overrides Ctrl+C. Press Ctrl+C. Verify it triggers the modal's cancel action — not process exit. Exit the modal, press Ctrl+C again — verify it now triggers the global exit handler.

## What to record

Key, handler component, guard predicate, coexisting handlers for same key, which wins when both are active.

## Done criteria

Every key handled by more than one component has mutual-exclusion guards verified. No two handlers for the same key can fire simultaneously without an `isActive` gate. All four blackbox probes pass with correct single-handler ownership at each UI layer.
