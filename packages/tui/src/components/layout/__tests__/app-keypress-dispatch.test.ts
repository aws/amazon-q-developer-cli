import { describe, it, expect, mock } from 'bun:test';
import {
  dispatchAppKeypress,
  type AppKeypressState,
  type AppKeypressActions,
} from '../app-keypress-dispatch';
import { parseKeybinding } from '../../../utils/keybindings';
import type { Key } from '../../../hooks/useKeypress';

// ---- Test helpers ----

const blankKey = (overrides: Partial<Key> = {}): Key => ({
  upArrow: false,
  downArrow: false,
  leftArrow: false,
  rightArrow: false,
  pageUp: false,
  pageDown: false,
  home: false,
  end: false,
  return: false,
  escape: false,
  ctrl: false,
  shift: false,
  meta: false,
  tab: false,
  backspace: false,
  delete: false,
  ...overrides,
});

const baseState = (
  overrides: Partial<AppKeypressState> = {}
): AppKeypressState => ({
  mode: 'inline',
  hasWorkflow: false,
  workflowInputActive: false,
  workflowHistoryOpen: false,
  activityTrayOpen: false,
  promptMenuOpen: false,
  isProcessing: false,
  isShellEscape: false,
  hasCommandInput: false,
  reverseSearchActive: false,
  pendingApproval: false,
  editingQueueIndex: null,
  editingSteerLineIndex: null,
  transientAlertHasAction: false,
  pendingOAuthUrl: null,
  subagentPanelOpen: false,
  surveyPromptVisible: false,
  suspendArmed: false,
  ...overrides,
});

function makeActions(): AppKeypressActions & {
  _calls: Record<string, number>;
  _args: Record<string, unknown[]>;
} {
  const calls: Record<string, number> = {};
  const args: Record<string, unknown[]> = {};
  const track =
    <T extends unknown[]>(name: string) =>
    (...a: T) => {
      calls[name] = (calls[name] ?? 0) + 1;
      args[name] = a;
    };
  return {
    cancelMessage: track('cancelMessage'),
    clearCommandInput: track('clearCommandInput'),
    resetExitSequence: track('resetExitSequence'),
    incrementExitSequence: track('incrementExitSequence'),
    armSuspend: track('armSuspend'),
    disarmSuspend: track('disarmSuspend'),
    setMode: track('setMode'),
    enterCrewMonitor: track('enterCrewMonitor'),
    enterWorkflowMonitor: track('enterWorkflowMonitor'),
    collapseActivityTray: track('collapseActivityTray'),
    fireTransientAlertAction: track('fireTransientAlertAction'),
    dismissTransientAlert: track('dismissTransientAlert'),
    copyOAuthUrl: track('copyOAuthUrl'),
    suspendProcess: track('suspendProcess'),
    shellEscapeWrite: mock(),
    acceptSurveyPrompt: track('acceptSurveyPrompt'),
    voiceCancel: null,
    _calls: calls,
    _args: args,
  };
}

const DEFAULT_BINDINGS = {
  quit: parseKeybinding('ctrl+c')!,
  cancelStream: parseKeybinding('esc')!,
};

// ---- cancelStream (esc) ----

describe('dispatchAppKeypress: cancelStream binding', () => {
  it('leaves global actions idle while a prompt menu owns input', () => {
    const actions = makeActions();
    const handled = dispatchAppKeypress(
      '',
      blankKey({ escape: true }),
      baseState({
        activityTrayOpen: true,
        isProcessing: true,
        promptMenuOpen: true,
      }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(handled).toBe(true);
    expect(actions._calls.collapseActivityTray).toBeUndefined();
    expect(actions._calls.cancelMessage).toBeUndefined();
  });

  it('keeps non-escape global shortcuts active while a prompt menu is open', () => {
    const actions = makeActions();
    const handled = dispatchAppKeypress(
      'c',
      blankKey({ ctrl: true }),
      baseState({ hasCommandInput: true, promptMenuOpen: true }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(handled).toBe(true);
    expect(actions._calls.clearCommandInput).toBe(1);
  });

  it('esc while streaming cancels the message', () => {
    const actions = makeActions();
    const handled = dispatchAppKeypress(
      '',
      blankKey({ escape: true }),
      baseState({ isProcessing: true }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(handled).toBe(true);
    expect(actions._calls.cancelMessage).toBe(1);
  });

  it('esc while idle does nothing (but is still handled)', () => {
    const actions = makeActions();
    const handled = dispatchAppKeypress(
      '',
      blankKey({ escape: true }),
      baseState({ isProcessing: false }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(handled).toBe(true);
    expect(actions._calls.cancelMessage).toBeUndefined();
  });

  it('esc during pending approval does NOT cancel (approval owns esc)', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      '',
      blankKey({ escape: true }),
      baseState({ isProcessing: true, pendingApproval: true }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._calls.cancelMessage).toBeUndefined();
  });

  it('esc while editing queue does NOT cancel streaming (queue owns esc)', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      '',
      blankKey({ escape: true }),
      baseState({ isProcessing: true, editingQueueIndex: 0 }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._calls.cancelMessage).toBeUndefined();
  });

  it('esc while editing a staged steer line does NOT cancel streaming (steer edit owns esc)', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      '',
      blankKey({ escape: true }),
      baseState({ isProcessing: true, editingSteerLineIndex: 0 }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._calls.cancelMessage).toBeUndefined();
  });

  it('esc while subagent panel is open does NOT cancel streaming', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      '',
      blankKey({ escape: true }),
      baseState({ isProcessing: true, subagentPanelOpen: true }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._calls.cancelMessage).toBeUndefined();
  });

  it('user-configured Ctrl+G cancels streaming when bound to cancelStream', () => {
    const actions = makeActions();
    const bindings = {
      ...DEFAULT_BINDINGS,
      cancelStream: parseKeybinding('ctrl+g')!,
    };
    dispatchAppKeypress(
      'g',
      blankKey({ ctrl: true }),
      baseState({ isProcessing: true }),
      actions,
      bindings
    );
    expect(actions._calls.cancelMessage).toBe(1);
  });

  it('esc does NOT cancel when user has rebound cancelStream to something else', () => {
    const actions = makeActions();
    const bindings = {
      ...DEFAULT_BINDINGS,
      cancelStream: parseKeybinding('ctrl+g')!,
    };
    dispatchAppKeypress(
      '',
      blankKey({ escape: true }),
      baseState({ isProcessing: true }),
      actions,
      bindings
    );
    expect(actions._calls.cancelMessage).toBeUndefined();
  });
});

// ---- quit (ctrl+c) ----

describe('dispatchAppKeypress: quit binding', () => {
  it('quit while idle on empty input starts the exit sequence', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      'c',
      blankKey({ ctrl: true }),
      baseState(),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._calls.incrementExitSequence).toBe(1);
  });

  it('quit while streaming cancels the message (NOT the exit sequence)', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      'c',
      blankKey({ ctrl: true }),
      baseState({ isProcessing: true }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._calls.cancelMessage).toBe(1);
    expect(actions._calls.incrementExitSequence).toBeUndefined();
  });

  it('quit with typed command input clears the input, does not exit', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      'c',
      blankKey({ ctrl: true }),
      baseState({ hasCommandInput: true }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._calls.clearCommandInput).toBe(1);
    expect(actions._calls.resetExitSequence).toBe(1);
    expect(actions._calls.incrementExitSequence).toBeUndefined();
  });

  it('quit in reverse search is a no-op (PromptInput handles it)', () => {
    const actions = makeActions();
    const handled = dispatchAppKeypress(
      'c',
      blankKey({ ctrl: true }),
      baseState({ reverseSearchActive: true }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(handled).toBe(true);
    expect(actions._calls.incrementExitSequence).toBeUndefined();
    expect(actions._calls.cancelMessage).toBeUndefined();
  });

  it('quit in crew-monitor / session-view is a no-op', () => {
    for (const mode of ['crew-monitor', 'session-view'] as const) {
      const actions = makeActions();
      dispatchAppKeypress(
        'c',
        blankKey({ ctrl: true }),
        baseState({ mode }),
        actions,
        DEFAULT_BINDINGS
      );
      expect(actions._calls.incrementExitSequence).toBeUndefined();
    }
  });

  it('user-configured Ctrl+Q starts the exit sequence when bound to quit', () => {
    const actions = makeActions();
    const bindings = {
      ...DEFAULT_BINDINGS,
      quit: parseKeybinding('ctrl+q')!,
    };
    dispatchAppKeypress(
      'q',
      blankKey({ ctrl: true }),
      baseState(),
      actions,
      bindings
    );
    expect(actions._calls.incrementExitSequence).toBe(1);
  });

  it('Ctrl+C does NOT start exit sequence when user has rebound quit elsewhere', () => {
    const actions = makeActions();
    const bindings = {
      ...DEFAULT_BINDINGS,
      quit: parseKeybinding('ctrl+q')!,
    };
    dispatchAppKeypress(
      'c',
      blankKey({ ctrl: true }),
      baseState(),
      actions,
      bindings
    );
    expect(actions._calls.incrementExitSequence).toBeUndefined();
  });

  it('double-press produces two increments (double-exit still required)', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      'c',
      blankKey({ ctrl: true }),
      baseState(),
      actions,
      DEFAULT_BINDINGS
    );
    dispatchAppKeypress(
      'c',
      blankKey({ ctrl: true }),
      baseState(),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._calls.incrementExitSequence).toBe(2);
  });
});

// ---- Hardcoded behaviors (not reconfigurable) ----

describe('dispatchAppKeypress: hardcoded behaviors', () => {
  it('first Ctrl+Z arms suspend, does not call suspendProcess', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      'z',
      blankKey({ ctrl: true }),
      baseState(),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._calls.armSuspend).toBe(1);
    expect(actions._calls.suspendProcess).toBeUndefined();
  });

  it('second Ctrl+Z calls disarmSuspend then suspendProcess', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      'z',
      blankKey({ ctrl: true }),
      baseState({ suspendArmed: true }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._calls.disarmSuspend).toBe(1);
    expect(actions._calls.suspendProcess).toBe(1);
  });

  it('Ctrl+Z does NOT suspend when subagent panel is open (panel claims it for jump-to-bottom)', () => {
    const actions = makeActions();
    const handled = dispatchAppKeypress(
      'z',
      blankKey({ ctrl: true }),
      baseState({ subagentPanelOpen: true }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(handled).toBe(false);
    expect(actions._calls.suspendProcess).toBeUndefined();
  });

  it('Ctrl+Z when picker open (pendingApproval) still arms suspend', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      'z',
      blankKey({ ctrl: true }),
      baseState({ pendingApproval: true }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._calls.armSuspend).toBe(1);
  });

  it('Ctrl+D on empty input starts exit sequence', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      'd',
      blankKey({ ctrl: true }),
      baseState(),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._calls.incrementExitSequence).toBe(1);
  });

  it('Ctrl+D while streaming does NOT exit (PromptInput handles it)', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      'd',
      blankKey({ ctrl: true }),
      baseState({ isProcessing: true }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._calls.incrementExitSequence).toBeUndefined();
  });

  it('Ctrl+D during shell escape is forwarded to PTY, not handled by dispatcher', () => {
    // In shell-escape mode, the AppContainer caller writes raw bytes to the
    // PTY directly; the dispatcher returns false for everything except
    // Ctrl+C (which has special cancel semantics). This matches the V1
    // behavior where the PTY's own terminal driver decides what Ctrl+D does
    // to the child process.
    const actions = makeActions();
    const handled = dispatchAppKeypress(
      'd',
      blankKey({ ctrl: true }),
      baseState({ isShellEscape: true }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(handled).toBe(false);
    expect(actions._calls.incrementExitSequence).toBeUndefined();
    expect(actions._calls.cancelMessage).toBeUndefined();
  });

  it('q leaves monitor and generic session views', () => {
    for (const mode of [
      'crew-monitor',
      'workflow-monitor',
      'session-view',
    ] as const) {
      const actions = makeActions();
      dispatchAppKeypress(
        'q',
        blankKey(),
        baseState({ mode }),
        actions,
        DEFAULT_BINDINGS
      );
      expect(actions._args.setMode).toEqual(['inline']);
    }
  });

  it('Ctrl+Y with transient alert action fires and dismisses', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      'y',
      blankKey({ ctrl: true }),
      baseState({ transientAlertHasAction: true }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._calls.fireTransientAlertAction).toBe(1);
    expect(actions._calls.dismissTransientAlert).toBe(1);
  });

  it('Ctrl+Y with pending OAuth copies the URL', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      'y',
      blankKey({ ctrl: true }),
      baseState({ pendingOAuthUrl: 'https://example.com/auth' }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._args.copyOAuthUrl).toEqual(['https://example.com/auth']);
  });

  it('Ctrl+G from inline enters crew-monitor', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      'g',
      blankKey({ ctrl: true }),
      baseState(),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._calls.enterCrewMonitor).toBe(1);
  });

  it('Ctrl+G from inline enters workflow monitor when a run is retained', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      'g',
      blankKey({ ctrl: true }),
      baseState({ hasWorkflow: true }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._calls.enterWorkflowMonitor).toBe(1);
    expect(actions._calls.enterCrewMonitor).toBeUndefined();
  });

  it('Ctrl+G from workflow monitor returns to inline', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      'g',
      blankKey({ ctrl: true }),
      baseState({ mode: 'workflow-monitor', hasWorkflow: true }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._args.setMode).toEqual(['inline']);
  });

  it('Tab from standalone agent monitor opens retained workflows', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      '',
      blankKey({ tab: true }),
      baseState({ mode: 'crew-monitor', hasWorkflow: true }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._calls.enterWorkflowMonitor).toBe(1);
  });

  it('workflow text input prevents global shortcuts from firing', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      'g',
      blankKey({ ctrl: true }),
      baseState({
        mode: 'workflow-monitor',
        hasWorkflow: true,
        workflowInputActive: true,
      }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._calls.enterWorkflowMonitor).toBeUndefined();
    expect(actions._calls.setMode).toBeUndefined();
  });

  it('workflow history prevents global shortcuts from firing underneath', () => {
    const actions = makeActions();
    const handled = dispatchAppKeypress(
      'g',
      blankKey({ ctrl: true }),
      baseState({
        hasWorkflow: true,
        workflowHistoryOpen: true,
      }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(handled).toBe(true);
    expect(actions._calls.enterWorkflowMonitor).toBeUndefined();
    expect(actions._calls.enterCrewMonitor).toBeUndefined();
    expect(actions._calls.setMode).toBeUndefined();
  });

  it('Esc collapses the activity tray without cancelling the active turn', () => {
    const actions = makeActions();
    const handled = dispatchAppKeypress(
      '',
      blankKey({ escape: true }),
      baseState({
        activityTrayOpen: true,
        isProcessing: true,
      }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(handled).toBe(true);
    expect(actions._calls.collapseActivityTray).toBe(1);
    expect(actions._calls.cancelMessage).toBeUndefined();
  });

  it('a retained expansion for a hidden tray does not claim Esc', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      '',
      blankKey({ escape: true }),
      baseState({
        activityTrayOpen: false,
        isProcessing: true,
      }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._calls.collapseActivityTray).toBeUndefined();
    expect(actions._calls.cancelMessage).toBe(1);
  });

  it('Ctrl+G from crew-monitor returns to inline', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      'g',
      blankKey({ ctrl: true }),
      baseState({ mode: 'crew-monitor' }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._args.setMode).toEqual(['inline']);
  });

  it('shell-escape Ctrl+C forwards to PTY and cancels', () => {
    const actions = makeActions();
    const handled = dispatchAppKeypress(
      'c',
      blankKey({ ctrl: true }),
      baseState({ isShellEscape: true }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(handled).toBe(true);
    expect(actions.shellEscapeWrite).toHaveBeenCalledWith('\x03');
    expect(actions._calls.cancelMessage).toBe(1);
    // Must NOT fall into the quit exit sequence path
    expect(actions._calls.incrementExitSequence).toBeUndefined();
  });
});

// ---- Exit-sequence reset behavior ----

describe('dispatchAppKeypress: exit sequence reset', () => {
  it('a plain printable key resets the exit sequence', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      'a',
      blankKey(),
      baseState(),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._calls.resetExitSequence).toBe(1);
  });

  it('a modifier-only combo we do not handle does NOT reset', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      'x',
      blankKey({ ctrl: true }),
      baseState(),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._calls.resetExitSequence).toBeUndefined();
  });

  it('a plain printable key also disarms suspend', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      'a',
      blankKey(),
      baseState({ suspendArmed: true }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._calls.disarmSuspend).toBe(1);
  });
});

describe('dispatchAppKeypress: suspend confirm cross-disarm', () => {
  it('Ctrl+D when suspendArmed calls incrementExitSequence (which cross-disarms)', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      'd',
      blankKey({ ctrl: true }),
      baseState({ suspendArmed: true }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._calls.incrementExitSequence).toBe(1);
  });

  it('quit binding when suspendArmed calls incrementExitSequence (which cross-disarms)', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      'c',
      blankKey({ ctrl: true }),
      baseState({ suspendArmed: true }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._calls.incrementExitSequence).toBe(1);
  });

  it('Ctrl+Z in crew-monitor still goes through confirm flow', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      'z',
      blankKey({ ctrl: true }),
      baseState({ mode: 'crew-monitor' }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._calls.armSuspend).toBe(1);
  });

  it('Ctrl+Z in session-view still goes through confirm flow', () => {
    const actions = makeActions();
    dispatchAppKeypress(
      'z',
      blankKey({ ctrl: true }),
      baseState({ mode: 'session-view' }),
      actions,
      DEFAULT_BINDINGS
    );
    expect(actions._calls.armSuspend).toBe(1);
  });
});
