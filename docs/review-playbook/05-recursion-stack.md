# Review 5 — Recursion and stack growth

**Why this class matters.** The TUI has deeply recursive code paths — yoga layout traverses the flexbox tree, the markdown parser recurses through nested blocks, the reconciler walks the fiber tree, tree diff is self-recursive. Any recursion without a bounded depth or an iterative fallback is a potential stack overflow on adversarial input (very deep nested lists, very nested JSX, circular refs).

**Scope.** Any function that calls itself or participates in a mutually-recursive cycle. This covers obvious cases — tree walks, AST visitors, reconciler commits, yoga layout, tree diff, deep-copy, JSON serialization with fallback, markdown/code parsers, path canonicalization with `..` resolution — and less-obvious ones like promise chains that re-enqueue themselves, `async` functions that recurse via `await`, and regexes with nested quantifiers (catastrophic backtracking is a stack-like resource exhaustion).

Concrete starting points: any parser or serializer, any `walk`/`visit`/`traverse`/`fold` helper, any renderer that recurses through children, and any regex used against user-supplied or agent-supplied text. New tree-walking code (future formatters, tree-sitter adapters, code-search tools) must be reviewed the same way.

## Techniques

1. **[code] Recursive-function census.** Open each tree-walk file and mark self-recursive functions. For each, document: what bounds the depth? User input, yoga tree depth, DOM depth, nesting in markdown AST?

2. **[code] Iterative-rewrite candidates.** For each recursive function whose depth is bounded by user input (markdown parser is the main one), note whether an iterative rewrite with an explicit stack is feasible. This is a future-work list, not a required fix.

3. **[code] Mutual-recursion scan.** Some recursion is mutual (A calls B calls A). Grep the call graph for cycles. These are harder to reason about than self-recursion.

4. **[code] Regex catastrophic-backtracking scan.** Grep for regexes with nested quantifiers: `(a+)+`, `(a|a)*`, `(.*)*`. Each is a potential ReDoS that can burn CPU without terminating. Candidates for technique 7.

5. **[blackbox] Adversarial input library.** Build a small corpus of hostile inputs: 1000-deep nested lists, 1000-deep nested blockquotes, a JSON with 10 000 nested objects, a markdown file of 50 000 lines with mixed code fences, a file path 4 096 bytes long, a file with 10 MB on a single line without newlines. Feed each through the relevant code path.

6. **[blackbox] `--stack-size` probe.** Run the TUI under `bun --stack-size=256` (or the bun equivalent) with the adversarial corpus. Any RangeError is a finding.

7. **[blackbox] Regex timeout fuzz.** For each regex identified in technique 4, run it against a pathological input like `"a".repeat(100) + "!"` with a 100 ms timeout. Anything that times out is a finding.

8. **[blackbox] Tree-depth fuzz.** Generate random JSX trees of depth 1 to 10 000 and render them; assert no RangeError and wall time < 1 s. The depth where the renderer breaks is the uncovered limit.

9. **[blackbox] Circular-reference probe.** Construct objects with reference cycles and pass them through every serialization / deep-copy / `JSON.stringify` path. Each should either handle the cycle (detect and reject) or short-circuit — never loop.

## What to record

Function, recursion bound, worst-case input, stack-size probe result, rewrite feasibility.

## Done criteria

Every recursion documents what bounds its depth. The adversarial-input corpus either completes cleanly or raises a handled error (no RangeError, no infinite loop). All four blackbox probes pass.
