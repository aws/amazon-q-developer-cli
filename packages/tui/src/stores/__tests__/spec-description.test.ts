/**
 * Store-level tests for the `/spec new` description-collection step:
 * arming the step, routing the next submitted line into the kickoff
 * prompt, the slash-command escape hatch, and esc cancellation. The step
 * renders from live state only — nothing is written to the transcript
 * until the user actually submits a description.
 */
import { describe, it, expect, mock } from 'bun:test';
import { createAppStore } from '../app-store';
import { composeSpecKickoffPrompt } from '../../utils/spec-workspace';

function makeStore() {
  const setConfigOption = mock(() => Promise.resolve());
  const kiro = {
    setConfigOption,
    isCloudSessionActive: () => false,
    recordSlashCommandInvocation: mock(),
    close: mock(),
  } as any;
  const store = createAppStore({ kiro });
  store.setState({ isInitialized: true, sessionId: 'test-session' });
  return { store, setConfigOption };
}

const PENDING = { featureName: 'slack bot' };

describe('spec description collection', () => {
  it('arming writes nothing to the transcript', () => {
    const { store } = makeStore();
    store.getState().setPendingSpecDescription(PENDING);
    expect(store.getState().pendingSpecDescription?.featureName).toBe(
      'slack bot'
    );
    expect(store.getState().messages).toHaveLength(0);
  });

  it('routes the next submitted line into the kickoff prompt', async () => {
    const { store } = makeStore();
    const sendMessage = mock(() => Promise.resolve());
    store.setState({
      pendingSpecDescription: PENDING,
      sendMessage: sendMessage as any,
      surveyPrompt: {
        message: 'How is it going?',
        survey: { id: 'session-feedback' },
      } as any,
    });

    await store.getState().handleUserInput('A bot that tracks design requests');

    expect(sendMessage).toHaveBeenCalledTimes(1);
    const [content, images, displayContent] = (
      sendMessage.mock.calls as any
    )[0];
    expect(content).toBe(
      composeSpecKickoffPrompt('slack bot', 'A bot that tracks design requests')
    );
    expect(content).toContain('ground truth');
    expect(images).toBeUndefined();
    expect(displayContent).toBe('A bot that tracks design requests');
    expect(store.getState().pendingSpecDescription).toBeNull();
    // The shared pre-send cleanup ran: a visible survey prompt is dismissed.
    expect(store.getState().surveyPrompt).toBeNull();
  });

  it('an informational command runs without costing the user their setup', async () => {
    const { store, setConfigOption } = makeStore();
    const sendMessage = mock(() => Promise.resolve());
    store.setState({
      pendingSpecDescription: PENDING,
      sendMessage: sendMessage as any,
      currentAgent: { name: 'spec' },
      agentEngine: 'kas',
      kasCommands: [{ name: '/help', description: '' }] as any,
    });

    await store.getState().handleUserInput('/help');

    // The command runs, and the step survives it: still in spec, still armed.
    expect(store.getState().showHelpPanel).toBe(true);
    expect(store.getState().pendingSpecDescription).toEqual(PENDING);
    expect(setConfigOption).not.toHaveBeenCalled();
    expect(sendMessage).not.toHaveBeenCalled();
  });

  it('any switch away from spec voids the step (commands, picker, shift+tab)', () => {
    const { store } = makeStore();
    store.setState({
      pendingSpecDescription: PENDING,
      currentAgent: { name: 'spec' },
    });

    // Every switch path funnels through setCurrentAgent.
    store.getState().setCurrentAgent({ name: 'kiro_planner' });

    expect(store.getState().pendingSpecDescription).toBeNull();
    expect(store.getState().transientAlert?.message).toContain('cancelled');
  });

  it('re-asserting spec (or arming under spec) does not void the step', () => {
    const { store } = makeStore();
    store.setState({
      pendingSpecDescription: PENDING,
      currentAgent: { name: 'spec' },
    });

    store.getState().setCurrentAgent({ name: 'spec' });

    expect(store.getState().pendingSpecDescription).toEqual(PENDING);
    expect(store.getState().transientAlert).toBeNull();
  });

  it('a description starting with a path is not mistaken for a command', async () => {
    const { store, setConfigOption } = makeStore();
    const sendMessage = mock(() => Promise.resolve());
    store.setState({
      pendingSpecDescription: PENDING,
      sendMessage: sendMessage as any,
      agentEngine: 'kas',
      kasCommands: [{ name: '/help', description: '' }] as any,
    });

    await store
      .getState()
      .handleUserInput('/Users/me/app should watch files and rebuild');

    // The step completes as a description: no cancel, no mode restore.
    expect(setConfigOption).not.toHaveBeenCalled();
    expect(store.getState().showHelpPanel).toBe(false);
    expect(store.getState().pendingSpecDescription).toBeNull();
    const [content, , displayContent] = (sendMessage.mock.calls as any)[0];
    expect(content).toBe(
      composeSpecKickoffPrompt(
        'slack bot',
        '/Users/me/app should watch files and rebuild'
      )
    );
    expect(displayContent).toBe('/Users/me/app should watch files and rebuild');
  });

  it('re-arming replaces the step (second /spec new mid-flow)', () => {
    const { store } = makeStore();
    store.getState().setPendingSpecDescription(PENDING);
    store.getState().setPendingSpecDescription({ featureName: 'other bot' });

    expect(store.getState().pendingSpecDescription).toEqual({
      featureName: 'other bot',
    });
  });

  it('cancel clears the step, keeps spec mode, and leaves no transcript trace', () => {
    const { store, setConfigOption } = makeStore();
    store.setState({
      pendingSpecDescription: PENDING,
      currentAgent: { name: 'spec' },
    });

    store.getState().cancelPendingSpecDescription();

    const state = store.getState();
    expect(state.pendingSpecDescription).toBeNull();
    // Mode is untouched: the user stays in spec and leaves it explicitly.
    expect(setConfigOption).not.toHaveBeenCalled();
    expect(state.currentAgent?.name).toBe('spec');
    expect(state.messages).toHaveLength(0);
    expect(state.transientAlert?.message).toContain('cancelled');
  });

  it('an images-only submit does not fire the kickoff with an empty description', async () => {
    const { store } = makeStore();
    const sendMessage = mock(() => Promise.resolve());
    store.setState({
      pendingSpecDescription: PENDING,
      sendMessage: sendMessage as any,
      pendingImages: [{ base64: 'abc', mimeType: 'image/png' }] as any,
    });

    await store.getState().handleUserInput('   ');

    expect(sendMessage).not.toHaveBeenCalled();
    // Step stays armed; the user is told to describe the spec in words.
    expect(store.getState().pendingSpecDescription).not.toBeNull();
    expect(store.getState().transientAlert?.status).toBe('warning');
  });

  it('cancelPendingSpecDescription is a no-op when nothing is pending', () => {
    const { store } = makeStore();
    store.getState().cancelPendingSpecDescription();
    expect(store.getState().transientAlert).toBeNull();
    expect(store.getState().messages).toHaveLength(0);
  });
});
