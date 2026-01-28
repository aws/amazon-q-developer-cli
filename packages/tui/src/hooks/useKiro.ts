import { useAppStore } from '../stores/app-store';

/**
 * Hook for ACP agent-specific state and actions.
 * Returns only Kiro/agent-related functionality.
 */
export const useKiro = () => {
  const isProcessing = useAppStore((state) => state.isProcessing);
  const agentError = useAppStore((state) => state.agentError);
  const kiro = useAppStore((state) => state.kiro);
  const sendMessage = useAppStore((state) => state.sendMessage);
  const cancelMessage = useAppStore((state) => state.cancelMessage);
  const setProcessing = useAppStore((state) => state.setProcessing);
  const setAgentError = useAppStore((state) => state.setAgentError);

  return {
    // Agent state
    isProcessing,
    error: agentError,
    isReady: !!kiro,

    // Agent actions
    sendMessage,
    cancel: cancelMessage,

    // Internal state setters (for event handlers)
    setProcessing,
    setError: setAgentError,
  };
};
