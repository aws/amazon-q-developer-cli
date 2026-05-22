---
id: 03-acp-lifecycle-code
review: 03-unbounded-growth
kind: code
partition: feature
scope:
  include:
    - "packages/tui/src/acp-client.ts"
    - "packages/tui/src/kiro.ts"
    - "packages/tui/src/utils/graceful-exit.ts"
    - "packages/tui/src/utils/turn-flush-machine.ts"
    - "packages/tui/src/utils/notification.ts"
  exclude:
    - "**/__tests__/**"
    - "**/*.test.*"
platform: any
harness: null
duration: fast
techniques: [1, 2, 5]
budget:
  input-tokens: 60000
  output-tokens: 8000
  wall-clock: 30m
findings-prefix: 03-acp-lifecycle-code
depends-on: []
defer: null
---

# Work item: Unbounded growth — ACP & session lifecycle — code

## Scope in one line

Audit the ACP client, session manager (kiro.ts), and lifecycle utilities for subscription leaks, unbounded Maps, and missing WeakMap opportunities.

## What to do

1. Read techniques 1, 2, 5 from `docs/review-playbook/03-unbounded-growth.md`.
2. Apply them to the files listed in `scope.include`, excluding `scope.exclude`.
3. **Technique 1 (Container census):** For every `new Map(`, `new Set(`, array, or `Record<string,` in these files, document: growth rate, trim/evict policy, and size bound.
4. **Technique 2 (Subscription leak scan):** Find every `.on(`, `.addEventListener`, `.subscribe`, `addHandler`, `onUpdate` call. Each must have a matching teardown reachable from the same owner (session end, process exit, component unmount). Unpaired registrations are findings.
5. **Technique 5 (WeakMap/WeakRef audit):** For any `Map` keyed by an object reference, check whether `WeakMap`/`WeakRef` would prevent GC retention.
6. Write one finding per issue under `docs/review-playbook/runner/findings/`.
7. Emit a done marker when complete.

## Budget rules

- If you finish under budget, do not expand scope.
- If you exceed budget before finishing, stop and emit a continuation marker.
- Never re-read the same file within a work item.
