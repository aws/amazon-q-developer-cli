---
id: 03-trim-caps-code-undo-stack-no-size-cap
work-item: 03-trim-caps-code
review: 03-unbounded-growth
technique: 4
class: missing-trim
severity: spiral
file: packages/twinki/packages/twinki/src/utils/undo-stack.ts
line: 10
platforms-affected: [all]
discovered-by: reviewer
discovered-at: 2026-05-05T00:10:00Z
status: open
---

# UndoStack grows unbounded — no size cap

The generic `UndoStack<S>` class stores deep-cloned state snapshots without any
maximum size. Each `push()` calls `structuredClone(state)`, storing a full copy
of the editor state. In a long editing session, every keystroke group produces a
snapshot. With no trim, memory grows linearly with edit count.

The `PromptInput.tsx` undo stack (which uses a plain array, not this class) caps
at 100 entries. The twinki `UndoStack` used by `Editor.ts` and `Input.ts` has
no such cap.

## Evidence

```typescript
// packages/twinki/packages/twinki/src/utils/undo-stack.ts:10
push(state: S): void {
    this.stack.push(structuredClone(state));  // No cap — grows forever
}
```

Compare with PromptInput.tsx which enforces a cap:
```typescript
// packages/tui/src/components/chat/prompt-bar/PromptInput.tsx:618
if (undoStack.current.length > 100) undoStack.current.shift();
```

## Proposed fix

Add a configurable cap with a sensible default (e.g., 200):

```typescript
export class UndoStack<S> {
    private stack: S[] = [];
    private readonly maxSize: number;

    constructor(maxSize = 200) {
        this.maxSize = maxSize;
    }

    push(state: S): void {
        this.stack.push(structuredClone(state));
        if (this.stack.length > this.maxSize) this.stack.shift();
    }
    // ...
}
```
