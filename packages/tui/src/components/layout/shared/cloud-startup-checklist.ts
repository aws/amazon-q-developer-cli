import chalk from 'chalk';

export interface CloudStartupChecklistState {
  connected: boolean;
  sessionCreated: boolean;
  /** Cloud session creation failed — renders a failed row instead of a spinner. */
  sessionFailed?: boolean;
  /** Display name of the connected source provider (e.g. "GitHub"), if any. */
  provider?: string;
  /**
   * Number of repositories the connected provider(s) expose, if known. When set
   * (and > 0), the final row reads `✓ N repositories found, /repo to select
   * (optional)`; when undefined the count is not yet known and the row falls
   * back to the bare `/repo to select (optional)` hint.
   */
  repoCount?: number;
}

export interface CloudStartupChecklistGlyphs {
  /** Done marker (Unicode `✓` / ASCII `+`), colored green when rendered. */
  check: string;
  /** Failure marker (Unicode `✗` / ASCII `x`), colored red when rendered. */
  cross: string;
  /** In-progress marker — the current spinner frame. */
  spinner: string;
  /** Pending-step suffix (Unicode `…` / ASCII `...`). */
  ellipsis: string;
}

/**
 * Cloud connect-screen checklist rows, shared by both layouts. Each bring-up
 * milestone is client-observable: completed steps render as a green `✓` line;
 * the first not-yet-complete step renders as a spinner + "…ing" line (so the
 * screen reads "Connecting to kiro.dev…" until the handshake lands, then ticks
 * to "✓ Connected to kiro.dev"). Later steps stay hidden until reached. Once
 * every step is done, the final row is `✓ N repositories found, /repo to select
 * (optional)` when the repo count is known, else a bare dim "/repo to select
 * (optional)" hint, followed by the "upload your local setup" guidance.
 */
export function formatCloudStartupChecklist(
  state: CloudStartupChecklistState,
  glyphs: CloudStartupChecklistGlyphs
): string[] {
  const done = (label: string): string =>
    `  ${chalk.green(glyphs.check)} ${label}`;
  const failed = (label: string): string =>
    `  ${chalk.red(glyphs.cross)} ${label}`;
  const pending = (label: string): string =>
    chalk.dim(`  ${glyphs.spinner} ${label}${glyphs.ellipsis}`);
  const rows: string[] = [];
  let spinnerShown = false;
  const step = (isDone: boolean, doneLabel: string, pendingLabel: string) => {
    if (isDone) {
      rows.push(done(doneLabel));
    } else if (!spinnerShown) {
      rows.push(pending(pendingLabel));
      spinnerShown = true;
    }
    // A not-yet-reached step (after the spinner) stays hidden until it starts.
  };
  step(state.connected, 'Connected to kiro.dev', 'Connecting to kiro.dev');
  // The provider line only exists once a connection is known, so it is always
  // a completed line (no pending form).
  if (state.provider) rows.push(done(`Connected to ${state.provider}`));
  if (state.sessionFailed) {
    // A failed create replaces the spinner with a terminal failed row (the
    // surrounding error alert carries the reason); no /repo hint follows.
    rows.push(failed('Cloud session failed'));
    return rows;
  }
  step(state.sessionCreated, 'Cloud session created', 'Creating cloud session');
  if (state.connected && state.sessionCreated) {
    const repoHint = `${chalk.magenta('/repo')} to select (optional)`;
    if (state.repoCount && state.repoCount > 0) {
      // A known count reads as a completed discovery step, matching the mock:
      // "✓ N repositories found, /repo to select (optional)".
      const noun = state.repoCount === 1 ? 'repository' : 'repositories';
      rows.push(done(`${state.repoCount} ${noun} found, ${repoHint}`));
    } else {
      rows.push(chalk.dim(`  ${repoHint}`));
    }
    // Guidance: the cloud workspace starts without the user's local ~/.kiro
    // config; point them at the web upload flow. Blank line then the paragraph.
    rows.push('');
    rows.push(
      chalk.dim(
        "  Your cloud workspace doesn't have your local setup yet. Go to " +
          `${chalk.magenta('kiro.dev/config/upload')} to bring your agents, MCP ` +
          'servers, hooks, and steering from ~/.kiro/ (home directory) to the cloud.'
      )
    );
  }
  return rows;
}
