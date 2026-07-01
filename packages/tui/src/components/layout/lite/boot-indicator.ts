/**
 * Boot-indicator helpers for the lite UI's footer chrome. Produces the
 * single dim row that surfaces in-flight async setup (agent_connect,
 * session_create, MCP loading) and disappears the moment its phase
 * settles.
 *
 * Replaces the previous multi-line connecting panel — see the
 * `showBootIndicator` memo's doc comment in LiteLayout.tsx for the
 * "why" of the redesign. This file owns the *what*: phase priority and
 * row formatting.
 */
import chalk from 'chalk';

export interface BootProgressEntry {
  label: string;
  status: string;
  startTime: number;
}

export interface McpInitEntry {
  status: string;
  startTime: number;
}

export interface BootIndicatorPhase {
  label: string;
  /** Milliseconds since the phase started loading. */
  elapsed: number;
}

/**
 * Pick the in-flight phase that should drive the dim footer row, or
 * null when nothing is loading. Priority:
 *
 *   1. `agent_connect` — the most fundamental blocker; without an ACP
 *      handshake nothing else can happen.
 *   2. `session_create` — the workspace-init RPC. Pre-MCP, but the user
 *      can already start typing and have their input queued.
 *   3. MCP aggregate — once the session is up, MCPs load in parallel.
 *      Reported as "Loading N/M MCP server(s)" with elapsed measured
 *      from the FIRST MCP that started loading, so a single slow server
 *      doesn't make the row's timer jump backwards as faster servers
 *      settle.
 *
 * Inputs are walked each call (no memo) — caller is expected to refresh
 * on every render so the elapsed value tracks real time.
 */
export function selectBootIndicatorPhase(
  bootProgress: Map<string, BootProgressEntry>,
  mcpInitStatus: Map<string, McpInitEntry>,
  now: number = Date.now()
): BootIndicatorPhase | null {
  const agentConnect = bootProgress.get('agent_connect');
  if (agentConnect?.status === 'loading') {
    return {
      label: 'Connecting to agent',
      elapsed: now - agentConnect.startTime,
    };
  }
  const sessionCreate = bootProgress.get('session_create');
  if (sessionCreate?.status === 'loading') {
    return {
      label: 'Initializing workspace',
      elapsed: now - sessionCreate.startTime,
    };
  }
  let loadingCount = 0;
  let totalCount = 0;
  let earliestStart = Infinity;
  for (const info of mcpInitStatus.values()) {
    totalCount++;
    if (info.status === 'loading') {
      loadingCount++;
      if (info.startTime < earliestStart) earliestStart = info.startTime;
    }
  }
  if (loadingCount > 0) {
    const settled = totalCount - loadingCount;
    return {
      label: `Loading ${settled}/${totalCount} MCP server(s)`,
      elapsed: now - earliestStart,
    };
  }
  return null;
}

/**
 * Render the dim boot-indicator row. Spinner glyph + label + elapsed
 * counter. Elapsed is suppressed below 1s so the row doesn't briefly
 * flash "(0.0s)" right when a phase starts. Returns '' when phase is
 * null so the caller's gate (`{showBootIndicator && ...}`) plus this
 * guard short-circuit cleanly if the maps drained between memo and
 * render.
 */
export function formatBootIndicator(
  phase: BootIndicatorPhase | null,
  spinChar: string,
  ellipsis: string = '…'
): string {
  if (!phase) return '';
  const elapsed =
    phase.elapsed > 1000 ? ` (${(phase.elapsed / 1000).toFixed(1)}s)` : '';
  return chalk.dim(`  ${spinChar} ${phase.label}${ellipsis}${elapsed}`);
}
