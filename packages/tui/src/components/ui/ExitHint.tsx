import React from 'react';
import { useAppStore } from '../../stores/app-store';
import { ActionHint } from './hint/ActionHint.js';

export const ExitHint: React.FC = () => {
  const exitSequence = useAppStore((state) => state.exitSequence);

  return (
    <ActionHint text="Press Ctrl+C again to exit" visible={exitSequence > 0} />
  );
};
