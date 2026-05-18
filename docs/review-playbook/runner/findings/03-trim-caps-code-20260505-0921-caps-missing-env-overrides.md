---
id: 03-trim-caps-code-20260505-0921-caps-missing-env-overrides
work-item: 03-trim-caps-code
review: 03-unbounded-growth
technique: 6
class: config-driven-caps
severity: smell
file: packages/tui/src/utils/command-history.ts
line: 7
platforms-affected: [all]
discovered-by: autoloop-reviewer
discovered-at: 2026-05-05T09:21:00Z
status: open
---

# Three size caps lack env-var overrides for stress-testing

`MAX_HISTORY_SIZE`, `maxSamples`, and `MAX_TAIL_SIZE` are hardcoded constants
with no environment-variable override. Unlike `MAX_STATIC_ITEMS` (which reads
`KIRO_MAX_STATIC_ITEMS`), these caps cannot be lowered at runtime to reproduce
memory-pressure scenarios or validate trim behaviour under stress.

## Evidence

```typescript
// packages/tui/src/utils/command-history.ts:7
const MAX_HISTORY_SIZE = 1000;

// packages/tui/src/utils/inputMetrics.ts:34
private maxSamples = 1000;

// packages/tui/src/utils/turn-flush-machine.ts:14
export const MAX_TAIL_SIZE = 6;
```

Compare with the pattern already established in `trim-static-items.ts`:

```typescript
// packages/tui/src/utils/trim-static-items.ts:3-5
export const MAX_STATIC_ITEMS = parseInt(
  process.env.KIRO_MAX_STATIC_ITEMS || '200',
  10
);
```

## Proposed fix

Apply the same `parseInt(process.env.KIRO_<NAME> || '<default>', 10)` pattern:

```typescript
// command-history.ts
const MAX_HISTORY_SIZE = parseInt(
  process.env.KIRO_MAX_HISTORY_SIZE || '1000',
  10
);

// inputMetrics.ts (in constructor or as module constant)
private maxSamples = parseInt(
  process.env.KIRO_MAX_INPUT_METRIC_SAMPLES || '1000',
  10
);

// turn-flush-machine.ts
export const MAX_TAIL_SIZE = parseInt(
  process.env.KIRO_MAX_TAIL_SIZE || '6',
  10
);
```
