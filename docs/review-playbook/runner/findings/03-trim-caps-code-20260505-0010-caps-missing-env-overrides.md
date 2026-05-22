---
id: 03-trim-caps-code-caps-missing-env-overrides
work-item: 03-trim-caps-code
review: 03-unbounded-growth
technique: 6
class: missing-config
severity: smell
file: packages/tui/src/stores/session-conversations.ts
line: 21
platforms-affected: [all]
discovered-by: reviewer
discovered-at: 2026-05-05T00:10:00Z
status: open
---

# Growth-relevant caps lack env-var overrides for stress testing

Four memory-growth-relevant caps are hard-coded constants with no env-var
override, making it impossible to stress-test trim behavior without code changes.
The reference pattern (`MAX_STATIC_ITEMS` in `trim-static-items.ts`) reads from
`process.env.KIRO_MAX_STATIC_ITEMS` — these caps should follow the same pattern.

## Evidence

```typescript
// packages/tui/src/stores/session-conversations.ts:21
const MAX_SESSION_MESSAGES = 50;  // No env-var override

// packages/tui/src/utils/command-history.ts:7
const MAX_HISTORY_SIZE = 1000;  // No env-var override

// packages/tui/src/components/chat/message/ShellOutputMessage.tsx:11
const MAX_EXPANDED_LINES = 1000;  // No env-var override

// packages/tui/src/utils/turn-flush-machine.ts:16
export const MAX_TAIL_SIZE = 6;  // No env-var override
```

Compare with the reference pattern:
```typescript
// packages/tui/src/utils/trim-static-items.ts:3
export const MAX_STATIC_ITEMS = parseInt(
  process.env.KIRO_MAX_STATIC_ITEMS || '200', 10
);
```

## Proposed fix

Add env-var overrides following the established pattern:

```typescript
const MAX_SESSION_MESSAGES = parseInt(
  process.env.KIRO_MAX_SESSION_MESSAGES || '50', 10
);
const MAX_HISTORY_SIZE = parseInt(
  process.env.KIRO_MAX_HISTORY_SIZE || '1000', 10
);
const MAX_EXPANDED_LINES = parseInt(
  process.env.KIRO_MAX_EXPANDED_LINES || '1000', 10
);
export const MAX_TAIL_SIZE = parseInt(
  process.env.KIRO_MAX_TAIL_SIZE || '6', 10
);
```
