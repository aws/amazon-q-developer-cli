export type AgentEngine = 'v2' | 'kas';

/**
 * Resolves the active agent engine from the environment. Read in places
 * that must determine the engine before the app store exists (binary
 * launch, ACP client construction). Once the store has booted prefer
 * `useAppStore.getState().agentEngine`.
 */
export function resolveAgentEngine(): AgentEngine {
  return process.env.KIRO_AGENT_ENGINE === 'kas' ? 'kas' : 'v2';
}

/** Subagent kill is V2-only; KAS (V3) has no working session/terminate. */
export function engineSupportsSubagentKill(engine: AgentEngine): boolean {
  return engine !== 'kas';
}

/** KAS exposes MCP snapshots and reset-server OAuth, but not `/mcp` actions. */
export function engineSupportsMcpCommandActions(engine: AgentEngine): boolean {
  return engine !== 'kas';
}
