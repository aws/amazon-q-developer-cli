import React from 'react';
import { InlineLayout } from './InlineLayout';
import { ExpandedLayout } from './ExpandedLayout';
import { CrewMonitorScreen } from './CrewMonitorScreen';
import { SessionViewScreen } from './SessionViewScreen';
import { useAppStore } from '../../stores/app-store';
import { useKeypress } from '../../hooks/useKeypress';

export const AppContainer: React.FC = () => {
  const mode = useAppStore((state) => state.mode);
  const setMode = useAppStore((state) => state.setMode);
  const incrementExitSequence = useAppStore(
    (state) => state.incrementExitSequence
  );
  const resetExitSequence = useAppStore((state) => state.resetExitSequence);
  const clearCommandInput = useAppStore((state) => state.clearCommandInput);
  const commandInputValue = useAppStore((state) => state.commandInputValue);
  const isProcessing = useAppStore((state) => state.isProcessing);
  const cancelMessage = useAppStore((state) => state.cancelMessage);
  const pendingApproval = useAppStore((state) => state.pendingApproval);

  const transientAlert = useAppStore((state) => state.transientAlert);
  const dismissTransientAlert = useAppStore(
    (state) => state.dismissTransientAlert
  );

  useKeypress((userInput, key) => {
    // Fire transient alert action on Ctrl+r
    if (key.ctrl && userInput === 'r' && transientAlert?.action) {
      transientAlert.action.onAction();
      dismissTransientAlert();
      return;
    }
    if (key.ctrl && userInput === 'c') {
      if (isProcessing) {
        cancelMessage();
      } else if (commandInputValue) {
        clearCommandInput();
        resetExitSequence();
      } else {
        incrementExitSequence();
      }
    } else if (key.ctrl && userInput === 'd') {
      // Ctrl+D only starts exit sequence when idle with empty input;
      // when there's text, PromptInput handles it as forward-delete.
      if (!isProcessing && !commandInputValue) {
        incrementExitSequence();
      }
    } else if (key.escape) {
      if (isProcessing && !pendingApproval) {
        cancelMessage();
      }
    } else if (
      !key.ctrl &&
      !key.meta &&
      userInput === 'q' &&
      (mode === 'crew-monitor' || mode === 'session-view')
    ) {
      process.stdout.write('\x1b[?1049l');
      setMode('inline');
    } else if (key.ctrl && userInput === 'g') {
      if (mode === 'crew-monitor') {
        process.stdout.write('\x1b[?1049l');
        setMode('inline');
      } else {
        process.stdout.write('\x1b[?1049h');
        setMode('crew-monitor');
      }
    } else if (!key.ctrl && !key.meta) {
      resetExitSequence();
    }
  });

  return (
    <>
      {mode === 'inline' && <InlineLayout />}
      {mode === 'expanded' && <ExpandedLayout />}
      {mode === 'crew-monitor' && <CrewMonitorScreen />}
      {mode === 'session-view' && <SessionViewScreen />}
    </>
  );
};
