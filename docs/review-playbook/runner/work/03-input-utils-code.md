---
id: 03-input-utils-code
review: 03-unbounded-growth
kind: code
partition: feature
scope:
  include:
    - "packages/tui/src/utils/input-editing.ts"
    - "packages/tui/src/types/input-buffer.ts"
    - "packages/tui/src/utils/command-history.ts"
    - "packages/tui/src/utils/reverse-search.ts"
    - "packages/tui/src/utils/path-completion.ts"
    - "packages/tui/src/utils/file-search.ts"
    - "packages/tui/src/utils/shell-escape.ts"
    - "packages/tui/src/utils/session-picker.ts"
    - "packages/tui/src/utils/sessions.ts"
  exclude:
    - "**/__tests__/**"
    - "**/*.test.*"
platform: any
harness: null
duration: fast
techniques: [1, 2, 3]
budget:
  input-tokens: 60000
  output-tokens: 8000
  wall-clock: 30m
findings-prefix: 03-input-utils-code
depends-on: []
defer: null
---

# Work item: Unbounded growth — Input editing & utility caches — code

## Scope in one line

Audit input-editing (undo/kill-ring), command history, path completion cache, file search, and session utilities for unbounded container growth and subscription leaks.

## What to do

1. Read techniques 1, 2, 3 from `docs/review-playbook/03-unbounded-growth.md`.
2. Apply them to the files listed in `scope.include`, excluding `scope.exclude`.
3. **Technique 1 (Container census):** For every `new Map(`, `new Set(`, array, or `Record<string,` in these files, document: growth rate, trim/evict policy, and size bound. Pay special attention to undo stacks, kill rings, completion caches, and session lists.
4. **Technique 2 (Subscription leak scan):** Find every `.on(`, `.subscribe`, event registration. Each must have a matching teardown. Unpaired registrations are findings.
5. **Technique 3 (Stream-buffer scan):** Identify any place that accumulates output or results without a cap (e.g., completion result arrays, search result buffers).
6. Write one finding per issue under `docs/review-playbook/runner/findings/`.
7. Emit a done marker when complete.

## Budget rules

- If you finish under budget, do not expand scope.
- If you exceed budget before finishing, stop and emit a continuation marker.
- Never re-read the same file within a work item.
