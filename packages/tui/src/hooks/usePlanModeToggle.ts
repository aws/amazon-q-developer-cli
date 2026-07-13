import { useAppStore } from '../stores/app-store.js';
import { useKeypress } from './useKeypress.js';
import { ModeChangeSource } from '../types/generated/chat-cli.js';

/** Shift+Tab toggles the active agent in/out of `kiro_planner` (plan mode). */
export function usePlanModeToggle(): void {
  const kiro = useAppStore((s) => s.kiro);
  const currentAgent = useAppStore((s) => s.currentAgent);
  const previousAgentName = useAppStore((s) => s.previousAgentName);
  const setCurrentAgent = useAppStore((s) => s.setCurrentAgent);
  const setPreviousAgentName = useAppStore((s) => s.setPreviousAgentName);
  const setLoadingMessage = useAppStore((s) => s.setLoadingMessage);
  const showTransientAlert = useAppStore((s) => s.showTransientAlert);

  useKeypress((_input, key) => {
    if (!(key.tab && key.shift)) return;
    const currentName = currentAgent?.name;
    const inPlan = currentName === 'kiro_planner';
    const target = inPlan ? previousAgentName : 'kiro_planner';
    if (!target) return;

    if (!inPlan && currentName) setPreviousAgentName(currentName);
    setLoadingMessage(`Agent changing to ${target}`);
    kiro
      .executeCommand({ command: 'agent', args: { agentName: target } })
      .then((result) => {
        setLoadingMessage(null);
        if (!result?.success) return;
        const name = (result.data as { agent?: { name?: string } })?.agent
          ?.name;
        if (currentName && name && currentName !== name) {
          kiro.sendModeChanged({
            fromMode: currentName,
            toMode: name,
            source: ModeChangeSource.ShiftTab,
            sessionId: kiro.sessionId,
          });
        }
        if (name) {
          setCurrentAgent({ name });
          showTransientAlert({
            message: `Switched to ${name}`,
            status: 'success',
            autoHideMs: 2000,
          });
        }
      })
      .catch(() => setLoadingMessage(null));
  });
}
