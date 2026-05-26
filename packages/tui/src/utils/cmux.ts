/**
 * cmux sidebar integration.
 *
 * When kiro-cli runs inside cmux (detected via CMUX_WORKSPACE_ID),
 * this module reports agent status to the cmux sidebar using the
 * `cmux set-status`, `cmux set-progress`, and `cmux log` commands.
 *
 * All calls are fire-and-forget — failures are silently ignored so
 * they never affect the chat experience.
 *
 * Opt-out: set KIRO_NO_CMUX=1 to disable.
 */

import { execFile, execFileSync } from 'node:child_process';
import { existsSync } from 'node:fs';

// ---------------------------------------------------------------------------
// Detection
// ---------------------------------------------------------------------------

const KNOWN_CMUX_PATHS = [
  '/Applications/cmux.app/Contents/Resources/bin/cmux',
  '/usr/local/bin/cmux',
];

/** Resolve the cmux binary path. Cached after first lookup. */
let _cmuxBin: string | null | undefined;
function getCmuxBin(): string | null {
  if (_cmuxBin !== undefined) return _cmuxBin;
  for (const p of KNOWN_CMUX_PATHS) {
    if (existsSync(p)) {
      _cmuxBin = p;
      return p;
    }
  }
  // Try PATH via `which`
  try {
    const resolved = execFileSync('which', ['cmux'], {
      timeout: 1000,
      encoding: 'utf-8',
      stdio: ['ignore', 'pipe', 'ignore'],
    }).trim();
    if (resolved) {
      _cmuxBin = resolved;
      return _cmuxBin;
    }
  } catch {
    /* not in PATH */
  }
  _cmuxBin = null;
  return null;
}

function isCmuxDisabled(): boolean {
  const v = process.env.KIRO_NO_CMUX;
  return (
    v !== undefined && v !== '' && v !== '0' && v.toLowerCase() !== 'false'
  );
}

let _insideCmux: boolean | null = null;

export function isInsideCmux(): boolean {
  if (_insideCmux === null) {
    _insideCmux =
      !isCmuxDisabled() &&
      !!process.env.CMUX_WORKSPACE_ID &&
      !!process.env.CMUX_SURFACE_ID &&
      getCmuxBin() !== null;
  }
  return _insideCmux;
}

// ---------------------------------------------------------------------------
// Low-level exec (fire-and-forget)
// ---------------------------------------------------------------------------

function cmux(...args: string[]): void {
  const bin = getCmuxBin();
  if (!bin || !isInsideCmux()) return;
  execFile(bin, args, { timeout: 3000 }, () => {
    /* ignore errors */
  });
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export type CmuxAgentStatus =
  | 'thinking'
  | 'tool-use'
  | 'compacting'
  | 'waiting-approval'
  | 'error'
  | 'idle';

let lastStatus: CmuxAgentStatus | null = null;

/**
 * Update the cmux sidebar to reflect the current agent state.
 * Deduplicates — won't re-send if status hasn't changed.
 */
export function syncCmuxStatus(status: CmuxAgentStatus, detail?: string): void {
  if (!isInsideCmux()) return;
  if (status === lastStatus && !detail) return;
  lastStatus = status;

  switch (status) {
    case 'thinking':
      cmux(
        'set-status',
        'agent',
        'Thinking…',
        '--icon',
        'brain',
        '--color',
        '#a78bfa'
      );
      cmux('set-progress', '0.5', '--label', detail ?? 'Generating response…');
      break;
    case 'tool-use':
      cmux(
        'set-status',
        'agent',
        detail ?? 'Running Tool',
        '--icon',
        'hammer.fill',
        '--color',
        '#f59e0b'
      );
      cmux('set-progress', '0.7', '--label', detail ?? 'Executing tool…');
      break;
    case 'compacting':
      cmux(
        'set-status',
        'agent',
        'Compacting',
        '--icon',
        'arrow.triangle.2.circlepath',
        '--color',
        '#60a5fa'
      );
      cmux('set-progress', '0.5', '--label', 'Compacting conversation…');
      break;
    case 'waiting-approval':
      cmux(
        'set-status',
        'agent',
        'Needs Approval',
        '--icon',
        'exclamationmark.triangle.fill',
        '--color',
        '#fbbf24'
      );
      cmux('clear-progress');
      break;
    case 'error':
      cmux(
        'set-status',
        'agent',
        'Error',
        '--icon',
        'xmark.circle.fill',
        '--color',
        '#ef4444'
      );
      cmux('clear-progress');
      break;
    case 'idle':
      cmux(
        'set-status',
        'agent',
        'Ready',
        '--icon',
        'checkmark.circle.fill',
        '--color',
        '#22c55e'
      );
      cmux('clear-progress');
      break;
  }
}

/** Log a message to the cmux sidebar log. */
export function cmuxLog(
  level: 'info' | 'success' | 'warning' | 'error',
  message: string
): void {
  if (!isInsideCmux()) return;
  cmux('log', '--level', level, '--', message);
}

/** Clean up cmux state on exit. */
export function cmuxCleanup(): void {
  if (!isInsideCmux()) return;
  cmux('clear-status', 'agent');
  cmux('clear-progress');
}
