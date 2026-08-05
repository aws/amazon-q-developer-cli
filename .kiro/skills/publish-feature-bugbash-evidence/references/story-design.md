# Multi-Step Story Design

Use this reference to turn feature requirements into a compact but
comprehensive bug-bash battery.

## Story Shape

Write each story as:

```text
ID: S01
Title: Continue the primary task while a secondary surface is open
Given: Exact state, data, active mode, and feature flags
When:
  1. First user action
  2. Asynchronous or lifecycle transition
  3. Second user action against the retained state
Then:
  - Intermediate state assertion
  - Final state assertion
  - Recovery or negative-control assertion
Matrix: engine, surface, viewport, OS when relevant
Evidence: named checks and frame labels
```

Avoid one-action stories when the defect class involves ownership, lifecycle,
focus, timing, or state composition. The failure often appears only after a
transition.

## Behavioral Angles

Select every angle the feature can plausibly affect:

1. **Happy path:** Complete the primary workflow once.
2. **Continuity:** Keep typing or interacting while a panel, tray, menu, task,
   output, or workflow remains visible.
3. **Alternation:** Repeat `act -> type -> act -> type`; prove ownership does not
   become stuck after the first transition.
4. **Multiplicity:** Create several messages, tasks, workflows, tabs, or tool
   outputs; move between them while preserving the draft and selection rules.
5. **Asynchronous updates:** Change backend state while the user is mid-input.
6. **Lifecycle edges:** Cover create, start, complete, remove-final-item,
   collapse, reopen, cancel, retry, and stale registration.
7. **Navigation:** Exercise bare arrows, modified arrows, Tab, Escape, and the
   feature shortcut in each relevant expanded or collapsed state.
8. **Destructive action:** Delete or cancel only the intended entity; prove a
   nonempty draft or unrelated selection is not consumed.
9. **Recovery:** Continue the primary task after deletion, completion, failure,
   or closing the secondary surface.
10. **Cross-surface parity:** Compare engines or UI surfaces only where the same
    contract is intended.
11. **Replay and remount:** Reconnect or remount the frontend while backend
    entities remain active; prove restored state is neither lost nor duplicated.
12. **Regression comparator:** Run the same story before and after the suspected
    introducing commit when attribution matters.
13. **Accessibility and layout:** Check keyboard-only operation, readable
    labels, narrow viewport wrapping, and absence of overlapping controls.

## Coverage Rules

- Map each requirement and reported symptom to story IDs before execution.
- Add a story for every distinct ownership or lifecycle boundary.
- Combine steps that form one real user journey; split unrelated behaviors.
- Include at least one nearby behavior that should remain unchanged.
- Derive draft, focus, and selection ownership from the existing product
  contract or prior behavior; do not invent per-entity ownership as a fix.
- Use exact inputs and expected state, not "works correctly."
- Record skipped matrix cells with a reason. Never count a skip as a pass.
- Stop expanding the matrix when additional cells share the same proven layer
  and add no independent risk.
