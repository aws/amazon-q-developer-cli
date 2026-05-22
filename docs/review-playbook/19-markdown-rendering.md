# Review 19 — Markdown rendering fidelity

**Why this class matters.** The TUI's markdown renderer has had 7+ bugs in a single month (Apr 2026) where specific markdown constructs render as raw text instead of styled output. Users see literal `**bold**`, `***` horizontal rules, or `+` list markers instead of formatted content. Each fix addresses one more edge case the parser missed, suggesting the line-by-line trigger logic is fragile and under-tested for the full CommonMark surface area.

**Scope.** The markdown rendering pipeline: `packages/tui/src/components/chat/markdown/` (MarkdownRenderer, inline parsing, block detection), table layout (`table-layout.ts`), and any component that renders assistant response content. Also covers the interaction between terminal width and content wrapping — tables, code blocks, and list items that hit exact-width boundaries.

For concrete starting points: `MarkdownRenderer.tsx`, `renderInlineText`, the line-by-line trigger regex, `table-layout.ts`, and the `needsSpacingBefore` logic for list items.

## Techniques

1. **[code] Trigger condition audit.** Read the line-by-line processing trigger in `MarkdownRenderer.tsx`. Enumerate every CommonMark block-level construct and verify each has a matching trigger regex. Flag any construct that would fall through to raw-text rendering.

2. **[code] Inline parsing coverage.** For each block type (blockquote, list item, table cell, heading), verify that `renderInlineText()` is called on the content. Prior bug: blockquotes rendered inline markdown as literal text because they used raw text directly.

3. **[blackbox] Full-construct rendering probe.** Send a prompt via Knight Rider that produces a response containing every markdown construct (headings, bold, italic, code, lists with `*`/`+`/`-`, numbered lists, blockquotes with inline styling, tables, horizontal rules `---`/`***`/`___`, fenced code blocks). Capture frames and verify no raw markers are visible in the rendered output.

4. **[blackbox] Table width boundary test.** Via Knight Rider, render a table with cells that sum to exactly the terminal width, then resize narrower. Verify the table wraps correctly without overflow or content loss.

5. **[blackbox] List de-indent spacing.** Render nested lists that transition between indent levels. Verify spacing appears when de-indenting (shallow → deep = tight, deep → shallow = spaced).

6. **[blackbox] Exact-width wrap boundary.** Render a list item whose text wraps at exactly the terminal width. Verify no blank line is swallowed at the boundary.

## What to record

For each finding: the markdown construct that fails, the trigger condition that missed it, the file and line where the gap exists, and a frame capture showing the raw markers in rendered output.

## Done criteria

Every CommonMark block-level construct has a matching trigger in the line-by-line regex. `renderInlineText()` is called for all block types that contain inline content. The Knight Rider probe passes with zero raw markers visible. Tables wrap correctly at all widths tested.
