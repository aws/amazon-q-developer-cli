import { useAppStore } from '../stores/app-store.js';
import { useKeypress } from './useKeypress.js';
import { extractRpcErrorMessage } from '../utils/error-handling.js';
import { ModeChangeSource } from '../types/generated/chat-cli.js';

/** Shift+Tab toggles the active agent in/out of `kiro_planner` (plan mode). */
export function usePlanModeToggle(isActive = true): void {
  const kiro = useAppStore((s) => s.kiro);
  const currentAgent = useAppStore((s) => s.currentAgent);
  const previousAgentName = useAppStore((s) => s.previousAgentName);
  const setCurrentAgent = useAppStore((s) => s.setCurrentAgent);
  const setPreviousAgentName = useAppStore((s) => s.setPreviousAgentName);
  const setLoadingMessage = useAppStore((s) => s.setLoadingMessage);
  const showTransientAlert = useAppStore((s) => s.showTransientAlert);

  useKeypress((_input, key) => {
    if (!isActive || !(key.tab && key.shift)) return;
    const currentName = currentAgent?.name;
    // No active agent yet (session still initializing): the primitive no-ops
    // without a session, so toggling here would falsely flip the chip.
    if (!currentName) return;
    const inPlan = currentName === 'kiro_planner';
    const target = inPlan ? previousAgentName : 'kiro_planner';
    if (!target) return;

    if (!inPlan) setPreviousAgentName(currentName);
    setLoadingMessage(`Agent changing to ${target}`);
    // A resolve means the swap landed (the primitive rejects on failure), so
    // set the chip optimistically rather than waiting for a store re-emit.
    kiro
      .setConfigOption('mode', target)
      .then(() => {
        setLoadingMessage(null);
        if (currentName !== target) {
          kiro.sendModeChanged({
            fromMode: currentName,
            toMode: target,
            source: ModeChangeSource.ShiftTab,
            sessionId: kiro.sessionId,
          });
        }
        setCurrentAgent({ name: target });
        showTransientAlert({
          message: `Switched to ${target}`,
          status: 'success',
          autoHideMs: 2000,
        });
      })
      .catch((err) => {
        setLoadingMessage(null);
        showTransientAlert({
          message: extractRpcErrorMessage(err, 'Failed to switch agent'),
          status: 'error',
          autoHideMs: 5000,
        });
      });
  });
}
