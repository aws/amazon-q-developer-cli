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
  // Truncation discoverability — /verbosity is no longer surfaced in
  // autocomplete; this tip is the primary discoverability vector for the
  // verbosity menu (alongside /settings).
  'Tune truncation, output filters, and density anytime via /settings → verbosity.',
  // Trust workflow — t opens scope picker on shell/fs, otherwise a confirm.
  'Press [t] on an approval prompt to trust scope (e.g. `git status` only) instead of the whole tool.',
  // Subagent panel — Ctrl+O is gated on a subagent being live so easy to miss.
  'Press Ctrl+O to inspect a running subagent live (cycle with Shift+←/→, scroll with ↑/↓).',
  // Mode swap — /tui is one of the fastest reactions to "lite is too sparse".
  'Type /tui to swap to the classic interface, /lite to come back. Scrollback is preserved.',
  // Theme — most users won't notice we have one. Tip surfaces it.
  'Type /theme to switch between Dark, Light, and Auto themes (Auto detects your terminal).',
  // Editing queue — common ask, easy to miss.
  'After queueing a message, ↑ pulls it back into the input to edit; empty + Enter deletes the slot.',
  // Interrupt — Esc + Ctrl+C are conventional but worth confirming.
  'Press Esc or Ctrl+C while Kiro is working to interrupt the current turn cleanly.',
  // /verbosity all toggle — recently shipped, not obvious.
  'In /settings → verbosity → Show output, the [all] row toggles on/off so one keystroke clears every filter.',
  // Output filters — explains why output bars sometimes don't appear.
  "If a tool's output isn't showing, /settings → verbosity → Show output controls which categories surface.",
  // Slash quickly — a lot of new users type the command name.
  'Just press / to open the command menu — search by typing, Tab to autocomplete.',
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
