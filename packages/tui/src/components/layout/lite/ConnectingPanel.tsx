import {
  getAgentColor,
  getAgentDisplayName,
} from '../../../utils/agentColors.js';
import { chalk } from '../../../utils/color.js';

// Footer indicator while an `/agent` RPC is in flight. Spinner + the target
// agent's name in its color — no extra label; the spinner-next-to-the-name
// reads as "pending" without saying "switching".
//
// `spinnerFrames` is required (not defaulted) because every caller threads
// the active braille set from `useSpinners()` so the chip respects
// /settings allowAsciiArt and the KIRO_ASCII_MODE env override. Defaulting
// to a hardcoded Unicode set would silently break ASCII mode on a future
// caller that forgets to pass frames.
//
// Note: this file used to host the multi-line connecting/MCP-loading panel
// (renderProgressLine + a spinner-frames default + an ms formatter). That
// panel was replaced in LiteLayout with a single-line dim boot indicator
// that picks the most relevant in-flight phase (agent_connect >
// session_create > MCP aggregate). See the `showBootIndicator` memo and
// `selectBootIndicatorPhase` helper there.
export function renderPendingAgent(
  pendingName: string,
  frame: number,
  getColor: (path: string) => any,
  spinnerFrames: readonly string[]
): string {
  const color = getAgentColor(pendingName, getColor);
  const spin = spinnerFrames[frame % spinnerFrames.length];
  return `${chalk.dim(spin)} ${color(getAgentDisplayName(pendingName))}`;
}
