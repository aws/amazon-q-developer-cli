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

  useKeypress((userInput, key) => {
    if (key.ctrl && userInput === 'c') {
      if (commandInputValue) {
        clearCommandInput();
        resetExitSequence();
      } else {
        incrementExitSequence();
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
