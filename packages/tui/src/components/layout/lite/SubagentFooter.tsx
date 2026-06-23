import chalk from 'chalk';
import { UNICODE_GLYPHS, type Glyphs } from '../../../utils/glyphs.js';
import { extractInlineArg } from '../../../lite/render.js';

export type SubagentPhase =
  | 'running'
  | 'summarizing'
  | 'complete'
  | 'requesting-permission'
  | 'killed';

export type SubagentRow = {
  name: string;
  phase: SubagentPhase;
  activeToolName: string | null;
  activeToolDetail: string | null;
  activeToolFinished: boolean;
};

const SUMMARY_TOOL_NAMES = new Set(['summary']);

export function isSubagentSummaryToolName(
  name: string | undefined | null
): boolean {
  return !!name && SUMMARY_TOOL_NAMES.has(name);
}

/**
 * Format one row of the subagent activity strip:
 *   "[stage-name] tool-name detail..."         (running)
 *   "[stage-name] Synthesizing..."              (summary tool in flight)
 *   "[stage-name] Thinking..."                  (running, no tool right now)
 *   "[stage-name] Requesting Permission: tool"  (subagent's tool is pending
 *                                                 approval at the parent prompt)
 *   "[stage-name] ✓ Complete"                   (summary tool finished)
 *
 * No spinner — the changing tool action text and the parent `subagent`
 * tool's ticking elapsed already convey "in flight". Rendered flush-left
 * at column 0 so the strip aligns with the rest of the footer chrome.
 * Truncated to the terminal width so the row always fits on one line.
 *
 * `glyphs` defaults to UNICODE_GLYPHS so existing callers and tests don't
 * need to wire it; LiteLayout passes the active set from `useGlyphs()` so
 * `✓ Complete` falls back to `+ Complete` in ASCII mode.
 */
export function formatSubagentRow(
  sub: SubagentRow,
  termCols: number,
  tagColor: (text: string) => string,
  glyphs: Glyphs = UNICODE_GLYPHS
): string {
  const tag = tagColor(`[${sub.name}]`);
  // Resolve the color once so the truncated branch below reuses it rather
  // than re-deriving the phase→color mapping.
  const colorFn =
    sub.phase === 'complete'
      ? chalk.green
      : sub.phase === 'killed'
        ? chalk.red
        : sub.phase === 'requesting-permission'
          ? // Yellow matches the approval prompt's tool-name color and the [t]
            // hotkey hint above the input — same trust signal, same color.
            chalk.yellow
          : chalk.dim;
  let plainAction: string;
  if (sub.phase === 'complete') {
    plainAction = `${glyphs.checkmark} Complete`;
  } else if (sub.phase === 'killed') {
    plainAction = `${glyphs.cross} killed`;
  } else if (sub.phase === 'requesting-permission') {
    plainAction = sub.activeToolName
      ? `Requesting Permission: ${sub.activeToolName}`
      : 'Requesting Permission';
  } else if (sub.phase === 'summarizing') {
    plainAction = 'Synthesizing...';
  } else if (sub.activeToolName) {
    const tail = sub.activeToolFinished ? '' : '...';
    const detail = sub.activeToolDetail ? ` ${sub.activeToolDetail}` : '';
    plainAction = `${sub.activeToolName}${detail}${tail}`;
  } else {
    plainAction = 'Thinking...';
  }
  const prefix = `[${sub.name}] `;
  const avail = Math.max(10, termCols - prefix.length);
  const action =
    plainAction.length > avail
      ? plainAction.slice(0, avail - 1) + '…'
      : plainAction;
  return `${tag} ${colorFn(action)}`;
}

/**
 * Build the one-line "detail" suffix shown in the footer activity strip
 * after the tool name. Delegates to the same {@link extractInlineArg}
 * formatter the lean density preset uses for inline tool-call chips, so
 * the footer mirrors the chat-log rendering (e.g. grep shows pattern + path,
 * write tools show verb + file, shell shows the command). The surrounding
 * `[...]` brackets are stripped since the tool name is shown separately by
 * {@link formatSubagentRow}. Returns null when nothing useful is available.
 */
export function extractFooterToolDetail(
  toolName: string,
  content: string
): string | null {
  if (!content) return null;
  // Reuse the lean preset's inline-arg formatter (no char cap — the footer
  // row is truncated to terminal width by formatSubagentRow anyway).
  const chip = extractInlineArg(toolName, content, null);
  if (!chip) return null;
  // Strip the surrounding [...] brackets that the inline-arg chip adds.
  if (chip.startsWith('[') && chip.endsWith(']')) {
    return chip.slice(1, -1);
  }
  return chip;
}
