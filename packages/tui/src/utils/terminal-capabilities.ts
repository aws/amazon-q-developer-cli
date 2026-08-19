import { isGhostty, isKitty } from './terminal-detection.js';

/**
 * Terminal capability detection and OSC escape sequence helpers.
 *
 * All terminal sniffing lives here so the rest of the codebase can call
 * `hasCapability(name)` instead of duplicating env-var checks.
 *
 * Opt-out env vars (set to "0" or "false" to disable):
 *   KIRO_NO_HYPERLINKS       — disable OSC 8 clickable links
 *   KIRO_NO_PROGRESS         — disable OSC 9;4 progress indicator
 *   KIRO_NO_SYNCHRONIZED     — disable DEC 2026 synchronized output
 */

// ---------------------------------------------------------------------------
// Capability detection
// ---------------------------------------------------------------------------

export type TerminalCapability =
  | 'synchronizedOutput'
  | 'hyperlinks'
  | 'progressIndicator';

let capabilityCache: Map<TerminalCapability, boolean> | null = null;

/** Returns true when the env var is set to any truthy value (present and not "0"/"false"). */
function envOptOut(name: string): boolean {
  const val = process.env[name];
  return (
    val !== undefined &&
    val !== '' &&
    val !== '0' &&
    val.toLowerCase() !== 'false'
  );
}

function tmuxSupportsSynchronizedOutput(
  termProgram: string,
  version: string
): boolean {
  if (termProgram !== 'tmux') return false;
  const match = version.match(/^(\d+)\.(\d+)/);
  if (!match) return false;
  const major = Number.parseInt(match[1]!, 10);
  const minor = Number.parseInt(match[2]!, 10);
  return major > 3 || (major === 3 && minor >= 7);
}

function buildCapabilityCache(): Map<TerminalCapability, boolean> {
  const cache = new Map<TerminalCapability, boolean>();

  if (!process.stdout.isTTY) {
    cache.set('synchronizedOutput', false);
    cache.set('hyperlinks', false);
    cache.set('progressIndicator', false);
    return cache;
  }

  const termProgram = process.env.TERM_PROGRAM ?? '';
  const termProgramVersion = process.env.TERM_PROGRAM_VERSION ?? '';
  const term = process.env.TERM ?? '';
  const terminalEmulator = process.env.TERMINAL_EMULATOR ?? '';
  const isTmux = !!process.env.TMUX;
  const isGnuScreen =
    !!process.env.STY || (!isTmux && /^screen(?:[.-]|$)/.test(term));

  // Synchronized output (DEC private mode 2026)
  let supportsSynchronizedOutput: boolean;
  if (envOptOut('KIRO_NO_SYNCHRONIZED')) {
    supportsSynchronizedOutput = false;
  } else if (isGnuScreen) {
    // Screen does not implement DEC 2026; outer-emulator hints are inherited.
    supportsSynchronizedOutput = false;
  } else if (isTmux) {
    // Outer-emulator variables leak through tmux, so only tmux's own pane
    // identity and version may enable DEC 2026. Unknown versions fail closed.
    supportsSynchronizedOutput = tmuxSupportsSynchronizedOutput(
      termProgram,
      termProgramVersion
    );
  } else {
    // Direct emulator: check emulator-specific signals.
    supportsSynchronizedOutput =
      termProgram === 'iTerm.app' ||
      termProgram === 'Alacritty' ||
      termProgram === 'WezTerm' ||
      termProgram === 'contour' ||
      termProgram === 'foot' ||
      isKitty() ||
      isGhostty();
  }
  cache.set('synchronizedOutput', supportsSynchronizedOutput);

  // OSC 8 hyperlinks
  const supportsHyperlinks =
    !envOptOut('KIRO_NO_HYPERLINKS') &&
    (termProgram === 'iTerm.app' ||
      termProgram === 'WezTerm' ||
      termProgram === 'Hyper' ||
      isGhostty() ||
      termProgram === 'Alacritty' ||
      termProgram === 'vscode' ||
      termProgram === 'foot' ||
      termProgram === 'contour' ||
      isKitty() ||
      terminalEmulator === 'JetBrains-JediTerm' ||
      process.env.WT_SESSION !== undefined ||
      process.env.VTE_VERSION !== undefined ||
      isTmux);
  cache.set('hyperlinks', supportsHyperlinks);

  // OSC 9;4 progress indicator (supported by iTerm2, WezTerm, Windows Terminal)
  const supportsProgress =
    !envOptOut('KIRO_NO_PROGRESS') &&
    (termProgram === 'iTerm.app' ||
      termProgram === 'WezTerm' ||
      process.env.WT_SESSION !== undefined); // Windows Terminal
  cache.set('progressIndicator', supportsProgress);

  return cache;
}

export function hasCapability(cap: TerminalCapability): boolean {
  if (!capabilityCache) {
    capabilityCache = buildCapabilityCache();
  }
  return capabilityCache.get(cap) ?? false;
}

/** Reset the capability cache (useful in tests). */
export function resetCapabilityCache(): void {
  capabilityCache = null;
}

// ---------------------------------------------------------------------------
// OSC 9;4 terminal progress indicator
// ---------------------------------------------------------------------------

/**
 * OSC 9;4 progress states (per iTerm2 spec):
 *   0 = hidden / remove indicator
 *   1 = normal / success (green) — requires percent, no percent = hidden
 *   2 = error (red) — with percent = static bar, without = pulsing/spinning
 *   3 = indeterminate (spinning green)
 *   4 = warning / paused (yellow) — requires percent, no percent = hidden
 */

function writeOsc94(state: 0 | 1 | 2 | 3 | 4, value?: number): void {
  if (!hasCapability('progressIndicator')) return;
  if (value !== undefined) {
    process.stdout.write(`\x1b]9;4;${state};${value}\x07`);
  } else {
    process.stdout.write(`\x1b]9;4;${state}\x07`);
  }
}

/** Spinning green — active processing. */
export function setTerminalProgressIndeterminate(): void {
  writeOsc94(3);
}

/** Static green bar at percent. */
export function setTerminalProgressNormal(percent: number): void {
  writeOsc94(1, Math.round(Math.max(0, Math.min(100, percent))));
}

/** Pulsing red — error. */
export function setTerminalProgressError(): void {
  writeOsc94(2);
}

/** Static yellow bar with percent. */
export function setTerminalProgressWarning(percent: number): void {
  writeOsc94(4, Math.round(Math.max(0, Math.min(100, percent))));
}

export function clearTerminalProgress(): void {
  writeOsc94(0);
}

// ---------------------------------------------------------------------------
// OSC 8 hyperlinks
// ---------------------------------------------------------------------------

/**
 * Wrap `text` in an OSC 8 hyperlink escape sequence when the terminal
 * supports it, otherwise return `text` unchanged.
 */
export function hyperlink(url: string, text: string): string {
  if (!hasCapability('hyperlinks')) return text;
  return `\x1b]8;;${url}\x07${text}\x1b]8;;\x07`;
}
