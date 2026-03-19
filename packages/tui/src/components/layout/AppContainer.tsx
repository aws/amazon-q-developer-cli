import React from 'react';
import { InlineLayout } from './InlineLayout';
import { ExpandedLayout } from './ExpandedLayout';
import { useAppStore } from '../../stores/app-store';
import { useKeypress } from '../../hooks/useKeypress';

export const AppContainer: React.FC = () => {
  const mode = useAppStore((state) => state.mode);
  const incrementExitSequence = useAppStore(
    (state) => state.incrementExitSequence
  );
  const resetExitSequence = useAppStore((state) => state.resetExitSequence);
  const clearCommandInput = useAppStore((state) => state.clearCommandInput);
  const commandInputValue = useAppStore((state) => state.commandInputValue);
  const isProcessing = useAppStore((state) => state.isProcessing);
  const cancelMessage = useAppStore((state) => state.cancelMessage);
  const pendingApproval = useAppStore((state) => state.pendingApproval);

  useKeypress((userInput, key) => {
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
    } else if (!key.ctrl && !key.meta) {
      resetExitSequence();
    }
  });

  switch (mode) {
    case 'expanded':
      return <ExpandedLayout />;
    case 'inline':
    default:
      return <InlineLayout />;
  }
};
