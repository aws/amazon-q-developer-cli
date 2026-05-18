---
id: 03-store-growth-code
review: 03-unbounded-growth
kind: code
partition: feature
scope:
  include:
    - "packages/tui/src/stores/app-store.ts"
    - "packages/tui/src/stores/selectors.ts"
    - "packages/tui/src/stores/session-conversations.ts"
    - "packages/tui/src/stores/message-stream-handler.ts"
    - "packages/tui/src/utils/feed-state.ts"
    - "packages/tui/src/utils/trim-static-items.ts"
    - "packages/tui/src/constants/feed.ts"
  exclude:
    - "**/__tests__/**"
    - "**/*.test.*"
    - "**/*.vitest.*"
platform: any
harness: null
duration: fast
techniques: [1, 3, 5]
budget:
  input-tokens: 60000
  output-tokens: 8000
  wall-clock: 30m
findings-prefix: 03-store-growth-code
depends-on: []
defer: null
---

# Work item: Unbounded growth — Store & state containers — code

## Scope in one line

Audit the zustand store, selectors, session-conversations, message-stream-handler, feed-state, and trim helpers for unbounded container growth and missing WeakMap opportunities.

## What to do

1. Read techniques 1, 3, 5 from `docs/review-playbook/03-unbounded-growth.md`.
2. Apply them to the files listed in `scope.include`, excluding `scope.exclude`.
3. **Technique 1 (Container census):** For every `new Map(`, `new Set(`, array declaration, or `Record<string,` in these files, document: growth rate (per-message / per-turn / per-session), trim/evict policy, and size bound.
4. **Technique 3 (Stream-buffer scan):** Identify any place streaming output is stored (`messages`, `toolCalls`, `liveOutputs`, `sessionLog`, etc.). Each must have a per-item cap, overall cap, or stream-to-file. Flag indefinite appenders.
5. **Technique 5 (WeakMap/WeakRef audit):** For any `Map` keyed by an object reference (React element, session object, etc.), check whether `WeakMap`/`WeakRef` would be more appropriate.
6. Write one finding per issue under `docs/review-playbook/runner/findings/`.
7. Emit a done marker when complete.

## Budget rules

- If you finish under budget, do not expand scope.
- If you exceed budget before finishing, stop and emit a continuation marker.
- Never re-read the same file within a work item.
