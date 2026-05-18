---
id: 03-hooks-render-code
review: 03-unbounded-growth
kind: code
partition: feature
scope:
  include:
    - "packages/tui/src/hooks/useConversationContent.ts"
    - "packages/tui/src/hooks/useExpandableOutput.ts"
    - "packages/tui/src/hooks/useKiro.ts"
    - "packages/tui/src/hooks/useScrollableBox.ts"
    - "packages/tui/src/hooks/useRenderMetrics.ts"
    - "packages/tui/src/hooks/useKeybindings.ts"
    - "packages/tui/src/hooks/useKeypress.ts"
    - "packages/tui/src/hooks/useTerminalSize.ts"
    - "packages/tui/src/utils/logger.ts"
    - "packages/tui/src/utils/terminal-capabilities.ts"
    - "packages/tui/src/utils/serialize-conversation.ts"
  exclude:
    - "**/__tests__/**"
    - "**/*.test.*"
platform: any
harness: null
duration: fast
techniques: [1, 2, 5]
budget:
  input-tokens: 40000
  output-tokens: 6000
  wall-clock: 20m
findings-prefix: 03-hooks-render-code
depends-on: []
defer: null
---

# Work item: Unbounded growth — Hooks & render utilities — code

## Scope in one line

Audit React hooks and render-adjacent utilities for subscription leaks on mount/unmount, unbounded container growth in refs, and missing WeakMap opportunities.

## What to do

1. Read techniques 1, 2, 5 from `docs/review-playbook/03-unbounded-growth.md`.
2. Apply them to the files listed in `scope.include`, excluding `scope.exclude`.
3. **Technique 1 (Container census):** For every `new Map(`, `new Set(`, `useRef(` holding a collection, or module-level cache, document: growth rate, trim/evict policy, and size bound.
4. **Technique 2 (Subscription leak scan):** Every `.on(`, `.subscribe`, `addEventListener` in a hook must have a matching cleanup in the effect's return function or equivalent teardown. Unpaired registrations are findings.
5. **Technique 5 (WeakMap/WeakRef audit):** For any `Map` keyed by a component instance, DOM node, or session object, check whether `WeakMap`/`WeakRef` would be more appropriate.
6. Write one finding per issue under `docs/review-playbook/runner/findings/`.
7. Emit a done marker when complete.

## Budget rules

- If you finish under budget, do not expand scope.
- If you exceed budget before finishing, stop and emit a continuation marker.
- Never re-read the same file within a work item.
