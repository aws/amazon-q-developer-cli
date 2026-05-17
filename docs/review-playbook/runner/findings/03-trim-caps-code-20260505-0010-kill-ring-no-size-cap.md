---
id: 03-trim-caps-code-kill-ring-no-size-cap
work-item: 03-trim-caps-code
review: 03-unbounded-growth
technique: 4
class: missing-trim
severity: spiral
file: packages/twinki/packages/twinki/src/utils/kill-ring.ts
line: 21
platforms-affected: [all]
discovered-by: reviewer
discovered-at: 2026-05-05T00:10:00Z
status: open
---

# KillRing grows unbounded — no size cap

The `KillRing` class stores every killed text entry without any maximum size.
In a long-running session with heavy Emacs-style editing (Ctrl+K, Alt+D, etc.),
the ring accumulates entries indefinitely. Each entry is a string that can be
arbitrarily large (e.g., killing an entire line of pasted content). Over hours
of editing, this constitutes unbounded memory growth with no trim policy.

## Evidence

```typescript
// packages/twinki/packages/twinki/src/utils/kill-ring.ts:21
push(text: string, opts: { prepend: boolean; accumulate?: boolean }): void {
    if (!text) return;

    if (opts.accumulate && this.ring.length > 0) {
        const last = this.ring.pop()!;
        this.ring.push(opts.prepend ? text + last : last + text);
    } else {
        this.ring.push(text);  // No cap check — grows forever
    }
}
```

No `MAX_KILL_RING_SIZE` constant exists. Compare with Editor.ts history which
caps at 100 entries (`if (this.history.length > 100) this.history.pop()`).

## Proposed fix

Add a size cap (e.g., 64 entries) and trim the oldest entry on push:

```typescript
private static readonly MAX_SIZE = 64;

push(text: string, opts: { prepend: boolean; accumulate?: boolean }): void {
    if (!text) return;
    if (opts.accumulate && this.ring.length > 0) {
        const last = this.ring.pop()!;
        this.ring.push(opts.prepend ? text + last : last + text);
    } else {
        this.ring.push(text);
        if (this.ring.length > KillRing.MAX_SIZE) this.ring.shift();
    }
}
```
