/**
 * Pure dispatch logic for top-level key handling in AppContainer.
 *
 * Extracted so the branching (quit vs cancel-stream vs mode-specific
 * shortcuts vs suspend) can be unit-tested without mounting the React tree.
 *
 * The AppContainer wires this up by calling {@link dispatchAppKeypress} from
 * inside a `useKeypress` handler with state from the Zustand store and
 * keybindings from {@link useKeybindings}.
 */
import type { Key } from '../../hooks/useKeypress.js';
import { matchesKeybinding, type Keybinding } from '../../utils/keybindings.js';
import type { AppMode } from '../../types/app-mode.js';

export interface AppKeypressState {
  mode: AppMode;
  hasWorkflow: boolean;
  workflowInputActive: boolean;
  workflowHistoryOpen: boolean;
  specReviewOpen: boolean;
  activityTrayOpen: boolean;
  promptMenuOpen: boolean;
  isProcessing: boolean;
  isShellEscape: boolean;
  hasCommandInput: boolean;
  reverseSearchActive: boolean;
  pendingApproval: boolean;
  editingQueueIndex: number | null;
  editingSteerLineIndex: number | null;
  commandInteractionOpen: boolean;
  transientAlertHasAction: boolean;
  pendingOAuthUrl: string | null;
  /**
   * Lite mode subagent inspection panel is open. When true, Esc and Ctrl+O
   * are claimed by the panel — don't fire stream cancel on Esc, and don't
   * let Ctrl+O trip any future top-level binding.
   */
  subagentPanelOpen: boolean;
  surveyPromptVisible: boolean;
  suspendArmed: boolean;
  /**
   * A goal loop is set on the session (active or paused). While true, the
   * quit key on an idle, empty prompt cancels the goal instead of arming the
   * exit sequence — otherwise mashing Ctrl+C to escape a goal walks straight
   * into process exit.
   */
  goalActive: boolean;
}

export interface AppKeypressActions {
  cancelMessage: () => void;
  cancelGoal: () => void;
  clearCommandInput: () => void;
  resetExitSequence: () => void;
  incrementExitSequence: () => void;
  armSuspend: () => void;
  disarmSuspend: () => void;
  setMode: (mode: AppMode) => void;
  enterCrewMonitor: () => void;
  enterWorkflowMonitor: () => void;
  /**
   * Reopen the most recently finished run in the workflow monitor. Returns
   * `true` if one was found (and the monitor was entered), `false` if there is
   * no archived run to reopen. Lets ctrl+g target a just-finished workflow that
   * `completeRun` has already evicted from the live `workflows` map.
   */
  enterWorkflowMonitorForLast: () => boolean;
  collapseActivityTray: () => void;
  fireTransientAlertAction: () => void;
  dismissTransientAlert: () => void;
  copyOAuthUrl: (url: string) => void;
  suspendProcess: () => void;
  shellEscapeWrite: ((bytes: string) => void) | null;
  acceptSurveyPrompt: () => void;
  voiceCancel: (() => void) | null;
}

export interface AppKeypressBindings {
  quit: Keybinding;
  cancelStream: Keybinding;
}

/**
 * Dispatch a key press. Returns `true` if the event was handled (and should
 * not fall through to other global handlers), `false` otherwise.
 *
 * Keeps the following behaviors hardcoded (not reconfigurable by the user):
 *   - Shell-escape Ctrl+C → PTY passthrough
 *   - Ctrl+Z → suspend
 *   - Ctrl+Y → transient alert action / OAuth URL copy
 *   - Ctrl+D → exit sequence
 *   - `q` in crew-monitor / session-view → back to inline
 *   - Ctrl+G → toggle crew-monitor
 */
export function dispatchAppKeypress(
  input: string,
  key: Key,
  state: AppKeypressState,
  actions: AppKeypressActions,
  bindings: AppKeypressBindings
): boolean {
  if (state.workflowHistoryOpen) return true;
  // The spec review surface covers the screen and owns every key it maps, so
  // no global shortcut may fire underneath it — a quit here would cancel the
  // checkpoint turn the surface exists to answer.
  if (state.specReviewOpen) return true;
  if (state.promptMenuOpen && key.escape) return true;

  if (state.mode === 'inline' && state.activityTrayOpen && key.escape) {
    actions.collapseActivityTray();
    return true;
  }

  // Shell-escape: forward everything to the PTY.
  if (state.isShellEscape && actions.shellEscapeWrite) {
    if (key.ctrl && input === 'c') {
      actions.shellEscapeWrite('\x03');
      actions.cancelMessage();
      return true;
    }
    // Caller is responsible for converting to raw bytes; the shell-escape
    // write itself happens at the call site because it needs keyToRawBytes.
    return false;
  }

  if (state.mode === 'workflow-monitor' && state.workflowInputActive) {
    return true;
  }

  // The dashboard owns every key while mounted, including modifiers used by
  // confirmation dialogs. Twinki broadcasts keypresses to active handlers.
  if (state.mode === 'session-dashboard') return false;

  if (key.ctrl && input === 'z') {
    // The lite subagent panel rebinds Ctrl+Z to "jump to bottom of trace".
    // Letting the global suspend fire here would background the process the
    // first time the user tried to jump down. Bail so the panel handler can
    // claim the keystroke (twinki delivers the same keypress to every active
    // useKeypress, so we just need to not return `true` here).
    if (state.subagentPanelOpen) return false;
    if (process.platform === 'win32') return true;

    if (state.suspendArmed) {
      actions.disarmSuspend();
      actions.suspendProcess();
    } else {
      actions.armSuspend();
    }
    return true;
  }

  if (key.ctrl && input === 'y' && state.transientAlertHasAction) {
    actions.fireTransientAlertAction();
    actions.dismissTransientAlert();
    return true;
  }

  if (key.ctrl && input === 'y' && state.surveyPromptVisible) {
    actions.acceptSurveyPrompt();
    return true;
  }

  if (key.ctrl && input === 'y' && state.pendingOAuthUrl) {
    actions.copyOAuthUrl(state.pendingOAuthUrl);
    return true;
  }

  if (matchesKeybinding(bindings.quit, input, key)) {
    if (
      state.mode === 'crew-monitor' ||
      state.mode === 'workflow-monitor' ||
      state.mode === 'session-view'
    ) {
      return true;
    }
    if (state.reverseSearchActive) {
      // PromptInput handles the quit key during reverse search
      return true;
    }
    if (actions.voiceCancel) {
      actions.voiceCancel();
    } else if (state.isProcessing) {
      actions.cancelMessage();
    } else if (state.hasCommandInput) {
      actions.clearCommandInput();
      actions.resetExitSequence();
    } else if (state.goalActive) {
      // A paused goal would resume on the next turn — a second Ctrl+C must
      // cancel it and hand back a clean prompt, not tick toward exit.
      actions.cancelGoal();
      actions.resetExitSequence();
    } else {
      actions.incrementExitSequence();
    }
    return true;
  }

  if (key.ctrl && input === 'd') {
    if (
      state.mode === 'crew-monitor' ||
      state.mode === 'workflow-monitor' ||
      state.mode === 'session-view'
    ) {
      return true;
    }
    if (state.isShellEscape) {
      actions.cancelMessage();
      actions.incrementExitSequence();
    } else if (!state.isProcessing && !state.hasCommandInput) {
      actions.incrementExitSequence();
    }
    return true;
  }

  if (matchesKeybinding(bindings.cancelStream, input, key)) {
    if (state.mode === 'workflow-monitor') return true;
    // Subagent panel claims Esc to close itself — don't piggyback a cancel.
    if (state.subagentPanelOpen) return true;
    if (state.commandInteractionOpen && key.escape) return true;
    if (
      state.isProcessing &&
      !state.pendingApproval &&
      state.editingQueueIndex == null &&
      state.editingSteerLineIndex == null
    ) {
      actions.cancelMessage();
    }
    return true;
  }

  if (
    !key.ctrl &&
    !key.meta &&
    input === 'q' &&
    (state.mode === 'crew-monitor' ||
      state.mode === 'workflow-monitor' ||
      state.mode === 'session-view')
  ) {
    actions.setMode('inline');
    return true;
  }

  if (key.tab && state.mode === 'crew-monitor' && state.hasWorkflow) {
    actions.enterWorkflowMonitor();
    return true;
  }

  if (key.ctrl && input === 'g') {
    if (state.mode === 'crew-monitor' || state.mode === 'workflow-monitor') {
      actions.setMode('inline');
    } else if (state.hasWorkflow) {
      actions.enterWorkflowMonitor();
    } else if (!actions.enterWorkflowMonitorForLast()) {
      // No live run and nothing to reopen — fall back to the crew monitor.
      actions.enterCrewMonitor();
    }
    return true;
  }

  // Reset exit sequence and disarm suspend on any non-modifier keypress that we didn't handle.
  if (!key.ctrl && !key.meta) {
    actions.resetExitSequence();
    actions.disarmSuspend();
  }
  return false;
}
