import React from 'react';
import { useAppStore } from '../../stores/app-store';
import { useKeybindings } from '../../hooks/useKeybindings.js';
import { ActionHint } from './hint/ActionHint.js';

export const ExitHint: React.FC = React.memo(function ExitHint() {
  const exitSequence = useAppStore((state) => state.exitSequence);
  const suspendArmed = useAppStore((state) => state.suspendArmed);
  const keybindings = useKeybindings();

  if (suspendArmed) {
    return (
      <ActionHint text="Press Ctrl+Z again to suspend" visible align="left" />
    );
  }

  return (
    <ActionHint
      text={`Press ${keybindings.label('quit')} or Ctrl+D again to exit`}
      visible={exitSequence > 0}
      align="left"
    />
  );
});
