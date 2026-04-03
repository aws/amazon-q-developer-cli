import React, { useEffect } from 'react';
import { InlineLayout } from './InlineLayout';
import { ExpandedLayout } from './ExpandedLayout';
import { CrewMonitorScreen } from './CrewMonitorScreen';
import { SessionViewScreen } from './SessionViewScreen';
import { useAppStore } from '../../stores/app-store';
import { useKeypress } from '../../hooks/useKeypress';
import {
  ENABLE_BRACKETED_PASTE,
  DISABLE_BRACKETED_PASTE,
  SHOW_CURSOR,
  HIDE_CURSOR,
  CLEAR_SCREEN,
} from '../../utils/terminal-sequences';

/**
 * Suspends the process by restoring terminal state and sending SIGTSTP
 * to the entire process group (Bun TUI + parent Rust process).
 */
function suspendProcess(): void {
  if (process.platform === 'win32') return;
  try {
    process.stdin.setRawMode?.(false);
    process.stdout.write(DISABLE_BRACKETED_PASTE);
    process.stdout.write(SHOW_CURSOR);
    process.stdout.write(
      '\nKiro CLI has been suspended. Run `fg` to resume.\n'
    );
  } catch {
    // stdin/stdout may not be available
  }
  process.kill(0, 'SIGTSTP');
}

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
  const isShellEscape = useAppStore((state) => state.isShellEscape);
  const cancelMessage = useAppStore((state) => state.cancelMessage);
  const pendingApproval = useAppStore((state) => state.pendingApproval);
  const editingQueueIndex = useAppStore((state) => state.editingQueueIndex);

  const transientAlert = useAppStore((state) => state.transientAlert);
  const dismissTransientAlert = useAppStore(
    (state) => state.dismissTransientAlert
  );

  // Restore terminal state when the process is resumed after ctrl+z suspend
  useEffect(() => {
    if (process.platform === 'win32') return;
    const handleCont = () => {
      try {
        process.stdin.setRawMode?.(true);
        process.stdout.write(ENABLE_BRACKETED_PASTE);
        process.stdout.write(HIDE_CURSOR);
      } catch {
        // stdin/stdout may not be available
      }
      // Write a clear sequence so twinki's stdout interceptor detects it
      // and triggers handleExternalClear() — a full redraw including static
      // scrollback content. SIGWINCH alone only redraws live content.
      process.stdout.write(CLEAR_SCREEN);
    };
    process.on('SIGCONT', handleCont);
    return () => {
      process.removeListener('SIGCONT', handleCont);
    };
  }, []);

  useKeypress((userInput, key) => {
    // Suspend process on Ctrl+Z
    if (key.ctrl && userInput === 'z') {
      suspendProcess();
      return;
    }
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
      // During shell escapes, allow Ctrl+D to cancel and exit.
      if (isShellEscape) {
        cancelMessage();
        incrementExitSequence();
      } else if (!isProcessing && !commandInputValue) {
        incrementExitSequence();
      }
    } else if (key.escape) {
      if (isProcessing && !pendingApproval && editingQueueIndex == null) {
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
