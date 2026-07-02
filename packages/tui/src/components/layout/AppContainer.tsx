import React, { useEffect } from 'react';
import { InlineLayout } from './InlineLayout';
import { ExpandedLayout } from './ExpandedLayout';
import { CrewMonitorScreen } from './CrewMonitorScreen';
import { SessionViewScreen } from './SessionViewScreen';
import { LiteLayout } from './lite/index.js';
import { TrustAllToolsGate } from '../ui/TrustAllToolsGate';
import { FirstLaunchUiModeGate } from '../ui/FirstLaunchUiModeGate';
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
import { startMcpOAuth } from '../../utils/mcp-oauth.js';
import { saveTrustGateAccepted } from '../../utils/trust-gate-state.js';
import { keyToRawBytes } from '../../hooks/useKeypress.js';
import { useKeybindings } from '../../hooks/useKeybindings.js';
import {
  dispatchAppKeypress,
  type AppKeypressState,
  type AppKeypressActions,
} from './app-keypress-dispatch.js';
import { AnimationPausedContext } from '../../contexts/AnimationPausedContext.js';
import { useAllowAnimations } from '../../hooks/useGlyphs.js';

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
  const uiMode = useAppStore((state) => state.uiMode);
  const trustAllToolsRequested = useAppStore(
    (state) => state.trustAllToolsRequested
  );
  const trustAllToolsConfirmed = useAppStore(
    (state) => state.trustAllToolsConfirmed
  );
  const confirmTrustAllTools = useAppStore(
    (state) => state.confirmTrustAllTools
  );
  const firstLaunchUiModeRequested = useAppStore(
    (state) => state.firstLaunchUiModeRequested
  );
  const confirmFirstLaunchUiMode = useAppStore(
    (state) => state.confirmFirstLaunchUiMode
  );
  const onExit = useAppStore((state) => state.onExit);
  const kiro = useAppStore((state) => state.kiro);
  const incrementExitSequence = useAppStore(
    (state) => state.incrementExitSequence
  );
  const resetExitSequence = useAppStore((state) => state.resetExitSequence);
  const armSuspend = useAppStore((state) => state.armSuspend);
  const disarmSuspend = useAppStore((state) => state.disarmSuspend);
  const suspendArmed = useAppStore((state) => state.suspendArmed);
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
  const agentEngine = useAppStore((state) => state.agentEngine);
  const showTransientAlert = useAppStore((state) => state.showTransientAlert);
  const subagentPanelOpen = useAppStore((state) => state.subagentPanelOpen);
  const surveyPrompt = useAppStore((state) => state.surveyPrompt);
  const openSurveyPanel = useAppStore((state) => state.openSurveyPanel);
  const voiceCancel = useAppStore((state) => state.voiceCancel);

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
      disarmSuspend();
      // Write a clear sequence so twinki's stdout interceptor detects it
      // and triggers handleExternalClear() — a full redraw including static
      // scrollback content. SIGWINCH alone only redraws live content.
      process.stdout.write(CLEAR_SCREEN);
    };
    process.on('SIGCONT', handleCont);
    return () => {
      process.removeListener('SIGCONT', handleCont);
    };
  }, [disarmSuspend]);

  const shellEscapeWriter = useAppStore((state) => state._shellEscapeWriter);

  const keybindings = useKeybindings();

  // useKeypress always invokes the latest handler closure (handlerRef), so the
  // values below are as fresh as the last render — no ref mirroring needed.
  useKeypress((userInput, key) => {
    // Shell-escape forwarding needs keyToRawBytes, which is TUI-specific.
    // Handle the "not Ctrl+C" case here; dispatchAppKeypress handles Ctrl+C.
    if (
      isShellEscape &&
      shellEscapeWriter &&
      !(key.ctrl && userInput === 'c')
    ) {
      shellEscapeWriter(keyToRawBytes(key, userInput));
      return;
    }

    const firstOAuthEntry =
      pendingOAuthServers.size > 0
        ? (pendingOAuthServers.entries().next().value as [string, string])
        : null;
    const firstOAuthUrl = firstOAuthEntry ? firstOAuthEntry[1] : null;

    const state: AppKeypressState = {
      mode,
      isProcessing,
      isShellEscape,
      hasCommandInput,
      reverseSearchActive,
      pendingApproval: !!pendingApproval,
      editingQueueIndex: editingQueueIndex ?? null,
      transientAlertHasAction: !!transientAlert?.action,
      pendingOAuthUrl: firstOAuthUrl,
      subagentPanelOpen,
      surveyPromptVisible: !!surveyPrompt,
      suspendArmed,
    };

    const actions: AppKeypressActions = {
      cancelMessage,
      clearCommandInput,
      resetExitSequence,
      incrementExitSequence,
      armSuspend,
      disarmSuspend,
      setMode,
      enterCrewMonitor: () => {
        // Enter alt screen immediately (before React re-renders) to prevent
        // CrewMonitorScreen content from polluting main screen scrollback.
        // useFullscreen() will sync twinki's internal altScreen flag on mount.
        process.stdout.write('\x1b[?1049h');
        setMode('crew-monitor');
      },
      fireTransientAlertAction: () => transientAlert?.action?.onAction(),
      dismissTransientAlert,
      acceptSurveyPrompt: () => {
        if (surveyPrompt) openSurveyPanel(surveyPrompt.survey);
      },
      copyOAuthUrl: (url) => {
        const serverName = firstOAuthEntry?.[0];
        if (!serverName) return;
        startMcpOAuth({
          agentEngine,
          serverName,
          url,
          resetMcpServer: (name, startOAuth) =>
            kiro.resetMcpServer(name, startOAuth),
          copyToClipboard: copyToSystemClipboard,
          showAlert: (message, status, autoHideMs) =>
            showTransientAlert({ message, status, autoHideMs }),
        });
      },
      suspendProcess,
      shellEscapeWrite: shellEscapeWriter ?? null,
      voiceCancel: voiceCancel ?? null,
    };

    dispatchAppKeypress(userInput, key, state, actions, keybindings);
  });

  const { allowAnimations } = useAllowAnimations();

  // Show trust-all-tools confirmation gate before allowing session to proceed
  if (trustAllToolsRequested && !trustAllToolsConfirmed) {
    return (
      <TrustAllToolsGate
        onAccept={confirmTrustAllTools}
        onAcceptAlways={() => {
          saveTrustGateAccepted(kiro);
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

  // First-launch UI mode picker. Shown only on a fresh install (no
  // chat.ui.mode persisted, no env var, no --lite/--tui CLI flag); index.tsx
  // sets the request flag after resolving the mode. Trust gate above wins
  // when both are set so users only see one block at a time.
  if (firstLaunchUiModeRequested) {
    return <FirstLaunchUiModeGate onPick={confirmFirstLaunchUiMode} />;
  }

  return (
    <AnimationPausedContext.Provider value={!allowAnimations}>
      {mode === 'inline' && uiMode === 'tui' && <InlineLayout />}
      {mode === 'inline' && uiMode === 'lite' && <LiteLayout />}
      {mode === 'expanded' && <ExpandedLayout />}
      {mode === 'crew-monitor' && <CrewMonitorScreen />}
      {mode === 'session-view' && <SessionViewScreen />}
    </AnimationPausedContext.Provider>
  );
};
