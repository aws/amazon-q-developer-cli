# `packages/tui-components` — Design Document

Status: Planning  
Date: 2026-05-11

---

## Purpose

`packages/tui-components` is a standalone workspace package that houses all UI components shared between modern and classic modes, along with a unified Storybook that browses both. It has no dependency on the Kiro backend, the ACP protocol, or the Zustand store. Components receive their data as props.

This separation serves three goals. First, components become independently testable and previewable without running the full TUI. Second, the Storybook covers both modern chrome (StatusBar, ToolUseMessage, ConversationView) and classic chrome (ClassicToolCall, ClassicApproval, ClassicLiveRegion) in one place, making visual regression easy to spot. Third, it establishes the extraction boundary for `packages/chat-classic` (the pure render functions) and for any future design system work.

---

## What Goes In

### From `packages/tui` (moved)

These components have no store dependencies and are pure presentational. They move to `tui-components` and are re-exported from `packages/tui` for backward compatibility.

**Shared primitives** (used by both modern and classic):
- `components/ui/spinner/` — `Spinner`, `PieSpinner`
- `components/ui/divider/` — `Divider`
- `components/ui/icon/` — `Icon`
- `components/ui/text/` — `Text` (styled wrapper)
- `components/ui/chip/` — `Chip`, `ProgressChip`
- `components/ui/alert/` — `Alert`, `BlockingErrorAlert`
- `components/ui/table/` — `Table`
- `components/ui/status/` — `StatusInfo`
- `components/ui/card/` — `Card`
- `components/ui/hint/` — `ActionHint`

**Modern-mode components** (moved, keep their stories):
- `components/chat/status-bar/` — `StatusBar`
- `components/chat/message/` — `Message`, `StreamingMessage`, `ThinkingMessage`
- `components/chat/tools/` — `Tool`, `Write`, `Read`, `Shell`, `Grep`, `Glob`, etc.
- `components/chat/prompt-bar/` — `PromptBar`, `ContextBar`, `SnackBar`, `PastedChip`
- `components/chat/notification-bar/` — `NotificationBar`
- `components/ui/radio/` — `RadioButton`, `RadioGroup`
- `components/ui/menu/` — `Menu`, `PromptsMenu`
- `components/brand/` — `Wordmark`
- `components/welcome-screen/` — `WelcomeScreen`

**Classic-mode components** (new, live here from the start):
- `classic/layout/ClassicToolCall`
- `classic/layout/ClassicApproval`
- `classic/layout/ClassicLiveRegion`
- `classic/layout/ClassicPromptLine`
- `classic/layout/ClassicInlineAutocomplete`

### What stays in `packages/tui`

Anything that reads from the Zustand store, uses `useAppStore`, or depends on `kiro.ts` / `acp-client.ts` stays in `packages/tui`. This includes:

- `InlineLayout`, `AppContainer`, `ClassicLayout` (the layout roots — they wire store to components)
- `ConversationView`, `VirtualScrollList` (depend on store selectors)
- `ApprovalRequest`, `TrustAllToolsGate` (depend on store actions)
- `CommandMenu` (depends on store slash command state)
- All panels (`HelpPanel`, `McpPanel`, `ToolsPanel`, etc.)
- `PromptInput` (depends on store input buffer state)

---

## Package Structure

```
packages/tui-components/
  package.json          name: "@kiro/tui-components", private: true
  tsconfig.json
  src/
    index.ts            re-exports everything public
    shared/             primitives used by both modern and classic
      spinner/
      divider/
      icon/
      text/
      chip/
      alert/
      table/
      status/
      card/
      hint/
    modern/             modern-mode presentational components
      status-bar/
      message/
      tools/
      prompt-bar/
      notification-bar/
      radio/
      menu/
      brand/
      welcome-screen/
    classic/            classic-mode presentational components (new)
      ClassicToolCall.tsx
      ClassicApproval.tsx
      ClassicLiveRegion.tsx
      ClassicPromptLine.tsx
      ClassicInlineAutocomplete.tsx
    storybook/
      run-storybook.tsx   entry point: bun run storybook
      Storybook.tsx       browser component (moved + extended)
      stories.ts          registry (moved + extended with classic stories)
```

---

## Storybook Organization

Stories are grouped into three top-level sections in the browser:

```
Shared
  └── Spinner, PieSpinner, Divider, Icon, Text, Chip, ProgressChip,
      Alert, Table, StatusInfo, Card, ActionHint

Modern
  └── StatusBar, Message, StreamingMessage, ThinkingMessage,
      Tool (Write, Read, Shell, Grep, Glob, …), PromptBar,
      ContextBar, SnackBar, NotificationBar, RadioButton,
      RadioGroup, Menu, Wordmark, WelcomeScreen

Classic
  └── ClassicToolCall (running, done, failed, subagent),
      ClassicApproval (pending, answered),
      ClassicLiveRegion (thinking, streaming, idle),
      ClassicPromptLine,
      ClassicInlineAutocomplete (slash, @-mention)
```

Each classic component has stories for every visual state it can be in. Because classic components are pure (props only, no store), stories are trivial to write.

---

## Dependencies

`packages/tui-components` depends on:
- `twinki` (workspace) — for `Box`, `Text`, `Static`, `useInput`, `render`
- `chalk` — for color utilities in classic render functions
- `react` — JSX

It does NOT depend on:
- `zustand`
- `@agentclientprotocol/sdk`
- `@kiro/client`
- Any file from `packages/tui/src/stores/`
- Any file from `packages/tui/src/kiro.ts` or `acp-client.ts`

This constraint is enforced by the `tsconfig.json` path configuration and optionally a lint rule.

`packages/tui` depends on `@kiro/tui-components` and re-exports everything for backward compatibility:

```ts
// packages/tui/src/components/ui/spinner/index.ts (after extraction)
export { Spinner, PieSpinner } from '@kiro/tui-components';
```

---

## Migration Strategy

The extraction happens in two phases to avoid a big-bang refactor.

**Phase 1 (Week 13, after classic mode ships):** Create the package skeleton, move `shared/` components only. These have the fewest dependencies and the lowest risk. Update `packages/tui` to re-export from the new package. Verify all existing tests pass.

**Phase 2 (Week 14):** Move `modern/` components. These are more numerous but still presentational. Update re-exports. Verify storybook runs from the new package location.

Classic components are written directly in `tui-components` from the start (during Weeks 5–9 of the classic mode build) — they never live in `packages/tui` at all.

---

## Storybook Launch

```jsonc
// packages/tui-components/package.json scripts
"storybook": "bun run src/storybook/run-storybook.tsx",
"storybook:classic": "STORYBOOK_FILTER=classic bun run src/storybook/run-storybook.tsx",
"storybook:modern": "STORYBOOK_FILTER=modern bun run src/storybook/run-storybook.tsx",
```

The `STORYBOOK_FILTER` env var filters the story list to a single section. This lets a developer working on classic components open only the classic stories without scrolling past the full modern component library.

The existing `packages/tui` storybook script (`bun run dev:storybook`) continues to work during the migration by importing from `@kiro/tui-components` once the package exists, or from the local path before it does.
