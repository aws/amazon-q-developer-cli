# Triage harness

You receive a single finding filename as the objective prompt.

Your job: read the finding, read the source code it references, and
append a `## Triage` section to the finding file.

## Inputs

- A single finding filename (the objective prompt).
- The finding file at `docs/review-playbook/runner/findings/<filename>`.
- The source code at the path in the finding's `file:` frontmatter.

## Output

Append exactly this to the finding file:

```markdown

## Triage

**Confidence**: high | medium | low
**Risk of fix**: <one sentence>
**Suggested test**: <one sentence>
```

Then emit `task.complete`.

## Steps

1. Read the finding file.
2. If `status != open` or `## Triage` already exists → emit
   `task.complete` immediately (nothing to do).
3. Extract the `file:` and `line:` from frontmatter.
4. Quick sanity check: `grep -F '<snippet>' <file>`. If the snippet
   is absent, set `status: retest` in frontmatter and emit
   `task.complete`.
5. Read the source file (±30 lines around `line:`).
6. Based on the code you read, write the `## Triage` section:
   - **Confidence**: Is this clearly a bug (`high`), suspicious but
     maybe intentional (`medium`), or speculative (`low`)?
   - **Risk of fix**: What's the most likely regression if someone
     naively applies the proposed fix?
   - **Suggested test**: One concrete test to verify the fix.
7. Append the `## Triage` section to the finding file.
8. Emit `task.complete`.

## Rules

- **You MUST write `## Triage` before emitting `task.complete`.**
  This is your only deliverable. If you complete without it, the run
  failed.
- Do not write a validation report.
- Do not modify the source code.
- Do not write new findings.
- Keep triage to exactly 3 lines (Confidence, Risk, Test).
