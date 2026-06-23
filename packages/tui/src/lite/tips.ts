/**
 * Lite-mode startup tips. One short tip is picked per session and rendered
 * just below the welcome banner — Claude Code does the same to surface
 * features that aren't obvious from the input box.
 *
 * Each tip is one sentence, lower-info-density on purpose: the first thing
 * a new user sees should not feel like a manual page. Tips are rotated so
 * frequent users keep learning without seeing the same nudge every time.
 *
 * Pure module — no React, no store reads. The lite layout calls {@link pickTip}
 * at mount and bakes the text into <Static> so it never re-renders.
 */
import chalk from 'chalk';

const TIPS: readonly string[] = [
  'Open /settings → verbosity to tune truncation, output filters, and density anytime.',
  'Choose which layout opens by default in /settings → display → Default UI (lite or tui).',
  'With the Ctrl+O inspect panel open, press Ctrl+X twice to kill a running subagent.',
  'Type /tui to swap to the classic interface and /lite to return — your conversation re-renders in the new style.',
  'Press Ctrl+O to watch a running subagent live — Shift+←/→ cycles subagents, Ctrl+A/Ctrl+Z jump to top/bottom.',
  'Inside /verbosity, Ctrl+P toggles a live preview and p expands it so you can see a change before committing.',
  'Pick a density preset (minimal, lean, default, full) in /verbosity to reset every output knob in one step.',
  "If a tool's output isn't showing, /settings → verbosity → Show output controls which categories surface.",
  'After queueing a message, ↑ pulls it back into the input to edit; empty + Enter deletes the slot.',
  'Switch between Auto, Dark, Light, and Custom themes via /settings → theme (Auto follows your terminal).',
  'Press Esc or Ctrl+C while Kiro is working to interrupt the current turn cleanly.',
];

/**
 * Pick a tip deterministically per startup. Daily rotation (no per-launch
 * thrash) so a user who restarts kiro five times in an hour still sees the
 * same tip — less jarring than a fresh nudge on every launch. The rotation
 * cycles through all tips over enough days that the user sees each one.
 */
export function pickTip(now: Date = new Date()): string {
  const dayOfYear = Math.floor(
    (now.getTime() - new Date(now.getFullYear(), 0, 0).getTime()) /
      (1000 * 60 * 60 * 24)
  );
  const idx = ((dayOfYear % TIPS.length) + TIPS.length) % TIPS.length;
  return TIPS[idx]!;
}

/**
 * Format a tip for the lite welcome banner. `Tip:` prefix in dim+bold so the
 * eye registers it as meta-text, not chat content; body is plain dim so it
 * doesn't compete with the user's first message.
 */
export function formatTipLine(tip: string): string {
  return `${chalk.dim.bold('  Tip:')} ${chalk.dim(tip)}`;
}

/** @internal Test-only access to the full tip list. */
export const __TIPS_FOR_TESTS = TIPS;
