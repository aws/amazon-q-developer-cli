/**
 * Store tests for the run a tasks-phase checkpoint defers.
 *
 * Answering the checkpoint has to reach the agent so it can close out the phase,
 * but the run itself cannot start until that turn is over — the answer returns
 * into a turn with no way to begin one. So the choice is recorded and started at
 * the turn boundary.
 */
import { describe, it, expect, mock } from 'bun:test';
import { createAppStore } from '../app-store';
import {
  RUN_REQUIRED_AND_OPTIONAL_TASKS,
  RUN_REQUIRED_TASKS,
} from '../../utils/spec-run-options';

function makeStore() {
  const kiro = {
    setConfigOption: mock(() => Promise.resolve()),
    isCloudSessionActive: () => false,
    sendChatSlashCommandTelemetry: mock(),
    close: mock(),
  } as any;
  const store = createAppStore({ kiro });
  store.setState({ isInitialized: true, sessionId: 'test-session' });
  return store;
}

/** A checkpoint on the given phase, with its question pending. */
function atCheckpoint(
  store: ReturnType<typeof makeStore>,
  phase: 'requirements' | 'tasks'
) {
  const question = {
    sessionId: 'test-session',
    toolCallId: 'q-1',
    question: 'Review the tasks, then:',
    options: [{ title: RUN_REQUIRED_TASKS }],
    resolve: mock(),
  } as any;
  store.setState({
    specPhaseCheckpoint: {
      featureName: 'web-clock',
      phase,
      artifactPath: `/w/.kiro/specs/web-clock/${phase}.md`,
      comments: [],
      review: null,
    },
    pendingQuestion: question,
    questionQueue: [question],
  });
  return question;
}

describe('a run chosen at the tasks checkpoint', () => {
  it('is held until the turn that asked for it ends', () => {
    const store = makeStore();
    const question = atCheckpoint(store, 'tasks');

    store.getState().respondToQuestion(RUN_REQUIRED_TASKS, question);

    // The agent still hears the answer, so the phase closes out normally.
    expect(question.resolve).toHaveBeenCalled();
    expect(store.getState().pendingSpecRun).toEqual({
      featureName: 'web-clock',
      makeAllRequired: false,
    });
  });

  it('carries the choice to promote optional tasks', () => {
    const store = makeStore();
    const question = atCheckpoint(store, 'tasks');

    store
      .getState()
      .respondToQuestion(RUN_REQUIRED_AND_OPTIONAL_TASKS, question);

    expect(store.getState().pendingSpecRun).toEqual({
      featureName: 'web-clock',
      makeAllRequired: true,
    });
  });

  it('holds nothing when the user declines', () => {
    const store = makeStore();
    const question = atCheckpoint(store, 'tasks');

    store.getState().respondToQuestion('Not now', question);

    expect(store.getState().pendingSpecRun).toBeNull();
  });

  it('holds nothing for typed feedback', () => {
    const store = makeStore();
    const question = atCheckpoint(store, 'tasks');

    store.getState().respondToQuestion('split task 3 in two', question);

    expect(store.getState().pendingSpecRun).toBeNull();
  });

  it('ignores a run title answered at another phase', () => {
    // Only the tasks phase offers a run; the same words elsewhere are feedback.
    const store = makeStore();
    const question = atCheckpoint(store, 'requirements');

    store.getState().respondToQuestion(RUN_REQUIRED_TASKS, question);

    expect(store.getState().pendingSpecRun).toBeNull();
  });

  it('ignores a run title with no checkpoint at all', () => {
    const store = makeStore();
    const question = {
      sessionId: 'test-session',
      toolCallId: 'q-2',
      question: 'Anything else?',
      options: [{ title: RUN_REQUIRED_TASKS }],
      resolve: mock(),
    } as any;
    store.setState({ pendingQuestion: question, questionQueue: [question] });

    store.getState().respondToQuestion(RUN_REQUIRED_TASKS, question);

    expect(store.getState().pendingSpecRun).toBeNull();
  });

  it('is discarded by the next turn if its own turn never reached the end', () => {
    // Cancelling or failing a turn returns before the boundary that starts the
    // run, so the request would otherwise survive to change files during a turn
    // the user began for something else.
    const store = makeStore();
    const question = atCheckpoint(store, 'tasks');
    store.getState().respondToQuestion(RUN_REQUIRED_TASKS, question);
    expect(store.getState().pendingSpecRun).not.toBeNull();

    void store.getState().sendMessage('something unrelated');

    expect(store.getState().pendingSpecRun).toBeNull();
  });
});
