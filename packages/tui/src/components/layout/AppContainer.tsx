import React, { useEffect } from 'react';
import { InlineLayout } from './InlineLayout';
import { ExpandedLayout } from './ExpandedLayout';
import { CrewMonitorScreen } from './CrewMonitorScreen';
import { SessionViewScreen } from './SessionViewScreen';
import { TrustAllToolsGate } from '../ui/TrustAllToolsGate';
import { useAppStore } from '../../stores/app-store';
import { useKeypress } from '../../hooks/useKeypress';
import {
  ENABLE_BRACKETED_PASTE,
  DISABLE_BRACKETED_PASTE,
  SHOW_CURSOR,
  HIDE_CURSOR,
  CLEAR_SCREEN,
} from '../../utils/terminal-sequences';
import { copyToSystemClipboard } from '../../commands/effects.js';
import { saveTrustGateAccepted } from '../../utils/trust-gate-state.js';

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
  const trustAllToolsRequested = useAppStore(
    (state) => state.trustAllToolsRequested
  );
  const trustAllToolsConfirmed = useAppStore(
    (state) => state.trustAllToolsConfirmed
  );
  const confirmTrustAllTools = useAppStore(
    (state) => state.confirmTrustAllTools
  );
  const onExit = useAppStore((state) => state.onExit);
  const kiro = useAppStore((state) => state.kiro);
  const incrementExitSequence = useAppStore(
    (state) => state.incrementExitSequence
  );
  const resetExitSequence = useAppStore((state) => state.resetExitSequence);
  const clearCommandInput = useAppStore((state) => state.clearCommandInput);
  const hasCommandInput = useAppStore((state) => !!state.commandInputValue);
  const isProcessing = useAppStore((state) => state.isProcessing);
  const isShellEscape = useAppStore((state) => state.isShellEscape);
  const cancelMessage = useAppStore((state) => state.cancelMessage);
  const reverseSearchActive = useAppStore((state) => state.reverseSearchActive);
  const pendingApproval = useAppStore((state) => state.pendingApproval);
  const editingQueueIndex = useAppStore((state) => state.editingQueueIndex);

  const transientAlert = useAppStore((state) => state.transientAlert);
  const dismissTransientAlert = useAppStore(
    (state) => state.dismissTransientAlert
  );
  const pendingOAuthServers = useAppStore((state) => state.pendingOAuthServers);
  const showTransientAlert = useAppStore((state) => state.showTransientAlert);

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
    // Fire transient alert action on Ctrl+y
    if (key.ctrl && userInput === 'y' && transientAlert?.action) {
      transientAlert.action.onAction();
      dismissTransientAlert();
      return;
    }
    // Copy OAuth URL on Ctrl+y when a server is pending auth
    if (key.ctrl && userInput === 'y' && pendingOAuthServers.size > 0) {
      const [, url] = pendingOAuthServers.entries().next().value as [
        string,
        string,
      ];
      if (copyToSystemClipboard(url)) {
        showTransientAlert({
          message: 'OAuth URL copied to clipboard',
          status: 'info',
          autoHideMs: 3000,
        });
      }
      return;
    }
    if (key.ctrl && userInput === 'c') {
      if (mode === 'crew-monitor' || mode === 'session-view') {
        return;
      }
      if (reverseSearchActive) {
        // PromptInput handles Ctrl+C during reverse search
        return;
      }
      if (isProcessing) {
        cancelMessage();
      } else if (hasCommandInput) {
        clearCommandInput();
        resetExitSequence();
      } else {
        incrementExitSequence();
      }
    } else if (key.ctrl && userInput === 'd') {
      // Don't trigger exit sequence on non-chat screens
      if (mode === 'crew-monitor' || mode === 'session-view') {
        return;
      }
      // Ctrl+D only starts exit sequence when idle with empty input;
      // when there's text, PromptInput handles it as forward-delete.
      // During shell escapes, allow Ctrl+D to cancel and exit.
      if (isShellEscape) {
        cancelMessage();
        incrementExitSequence();
      } else if (!isProcessing && !hasCommandInput) {
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
      // Don't manually write \x1b[?1049l here — CrewMonitorScreen's
      // useFullscreen() hook handles alt screen exit on unmount.
      // Writing it manually causes a double exit which resets keyboard
      // mode state in terminals with Kitty protocol, breaking Option key.
      setMode('inline');
    } else if (key.ctrl && userInput === 'g') {
      if (mode === 'crew-monitor') {
        setMode('inline');
      } else {
        // Enter alt screen immediately (before React re-renders) to prevent
        // CrewMonitorScreen content from polluting main screen scrollback.
        // useFullscreen() will sync twinki's internal altScreen flag on mount.
        process.stdout.write('\x1b[?1049h');
        setMode('crew-monitor');
      }
    } else if (!key.ctrl && !key.meta) {
      resetExitSequence();
    }
  });

  // Show trust-all-tools confirmation gate before allowing session to proceed
  if (trustAllToolsRequested && !trustAllToolsConfirmed) {
    return (
      <TrustAllToolsGate
        onAccept={confirmTrustAllTools}
        onAcceptAlways={() => {
          saveTrustGateAccepted();
          confirmTrustAllTools();
        }}
        onExit={() => {
          kiro.close();
          onExit?.();
          process.exit(0);
        }}
      />
    );
  }

  return (
    <>
      {mode === 'inline' && <InlineLayout />}
      {mode === 'expanded' && <ExpandedLayout />}
      {mode === 'crew-monitor' && <CrewMonitorScreen />}
      {mode === 'session-view' && <SessionViewScreen />}
    </>
  );
};
