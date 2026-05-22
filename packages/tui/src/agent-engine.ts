export type AgentEngine = 'rust' | 'kas';

/**
 * Resolves the active agent engine from the environment. Read in places
 * that must determine the engine before the app store exists (binary
 * launch, ACP client construction). Once the store has booted prefer
 * `useAppStore.getState().agentEngine`.
 */
export function resolveAgentEngine(): AgentEngine {
  return process.env.KIRO_AGENT_ENGINE === 'kas' ? 'kas' : 'rust';
}
