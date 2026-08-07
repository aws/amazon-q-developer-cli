/**
 * Integration tests for the research-survey flow at the store level.
 *
 * These tests exercise the full lifecycle:
 *   1. Turn counting → notification trigger
 *   2. Guard conditions (queued messages, pending tasks, etc.)
 *   3. Opening the panel → submitting → Aperture call
 *   4. Dismissal flow
 *
 * We mock:
 *   - Kiro (no real ACP connection)
 *   - fetch (no real network)
 *   - survey-state persistence (isolated KIRO_HOME per test)
 */
import {
  describe,
  it,
  expect,
  mock,
  beforeEach,
  afterEach,
  afterAll,
} from 'bun:test';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createAppStore } from '../app-store';
import { Kiro } from '../../kiro';

// Mock Kiro
mock.module('../../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    cancel: mock(),
    close: mock(),
  })),
}));

afterAll(() => {
  mock.restore();
});

describe('Survey flow integration', () => {
  let tmpDir: string;
  let originalKiroHome: string | undefined;
  let originalSampleRate: string | undefined;
  let originalCooldown: string | undefined;
  let originalFetch: typeof fetch;

  beforeEach(() => {
    // Isolate survey state persistence
    originalKiroHome = process.env.KIRO_HOME;
    originalSampleRate = process.env.KIRO_SURVEY_SAMPLE_RATE;
    originalCooldown = process.env.KIRO_SURVEY_COOLDOWN_DAYS;
    originalFetch = globalThis.fetch;

    tmpDir = mkdtempSync(join(tmpdir(), 'kiro-survey-integ-'));
    process.env.KIRO_HOME = tmpDir;
    // Force 100% eligibility and 0-day cooldown for deterministic tests
    process.env.KIRO_SURVEY_SAMPLE_RATE = '1';
    process.env.KIRO_SURVEY_COOLDOWN_DAYS = '0';

    // Mock fetch to succeed silently
    globalThis.fetch = mock(
      async () => new Response('{"id":"test-id"}', { status: 200 })
    ) as unknown as typeof fetch;
  });

  afterEach(() => {
    if (originalKiroHome === undefined) delete process.env.KIRO_HOME;
    else process.env.KIRO_HOME = originalKiroHome;
    if (originalSampleRate === undefined)
      delete process.env.KIRO_SURVEY_SAMPLE_RATE;
    else process.env.KIRO_SURVEY_SAMPLE_RATE = originalSampleRate;
    if (originalCooldown === undefined)
      delete process.env.KIRO_SURVEY_COOLDOWN_DAYS;
    else process.env.KIRO_SURVEY_COOLDOWN_DAYS = originalCooldown;
    globalThis.fetch = originalFetch;
    rmSync(tmpDir, { recursive: true, force: true });
  });

  function makeStore() {
    const store = createAppStore({ kiro: new Kiro() });
    store.setState({ isInitialized: true });
    return store;
  }

  it('does not show survey before 3 completed turns', () => {
    const store = makeStore();
    store.getState().recordCompletedTurn();
    store.getState().recordCompletedTurn();
    expect(store.getState().transientAlert).toBeNull();
    expect(store.getState().completedTurnCount).toBe(2);
  });

  it('shows survey notification after 3 completed turns', () => {
    const store = makeStore();
    store.getState().recordCompletedTurn();
    store.getState().recordCompletedTurn();
    store.getState().recordCompletedTurn();
    expect(store.getState().surveyPrompt).not.toBeNull();
    expect(store.getState().surveyPrompt!.message).toBe('How did Kiro do?');
  });

  it('does not show survey when queued messages exist', () => {
    const store = makeStore();
    store.setState({ queuedMessages: ['pending prompt'] });
    store.getState().recordCompletedTurn();
    store.getState().recordCompletedTurn();
    store.getState().recordCompletedTurn();
    expect(store.getState().transientAlert).toBeNull();
  });

  it('does not show survey when a pending steer message exists', () => {
    const store = makeStore();
    store.setState({ pendingSteerContent: 'pending prompt' });
    store.getState().recordCompletedTurn();
    store.getState().recordCompletedTurn();
    store.getState().recordCompletedTurn();
    expect(store.getState().transientAlert).toBeNull();
  });

  it('does not show survey when pending tasks exist', () => {
    const store = makeStore();
    store.setState({
      tasks: [{ id: '1', subject: 'Do something', status: 'pending' }],
    });
    store.getState().recordCompletedTurn();
    store.getState().recordCompletedTurn();
    store.getState().recordCompletedTurn();
    expect(store.getState().transientAlert).toBeNull();
  });

  it('does not show survey when isProcessing is true', () => {
    const store = makeStore();
    store.setState({ isProcessing: true });
    store.getState().recordCompletedTurn();
    store.getState().recordCompletedTurn();
    store.getState().recordCompletedTurn();
    expect(store.getState().transientAlert).toBeNull();
  });

  it('does not show survey when a transient alert is already showing', () => {
    const store = makeStore();
    store.setState({
      transientAlert: { message: 'existing', status: 'info' },
    });
    store.getState().recordCompletedTurn();
    store.getState().recordCompletedTurn();
    store.getState().recordCompletedTurn();
    // Should not overwrite the existing alert
    expect(store.getState().transientAlert!.message).toBe('existing');
  });

  it('openSurveyPanel sets showSurveyPanel and clears the prompt bar', () => {
    const store = makeStore();
    // Trigger the notification first
    store.getState().recordCompletedTurn();
    store.getState().recordCompletedTurn();
    store.getState().recordCompletedTurn();
    expect(store.getState().surveyPrompt).not.toBeNull();

    // Open the panel (simulates Shift+R action)
    store.getState().openSurveyPanel(store.getState().surveyPrompt!.survey);
    expect(store.getState().showSurveyPanel).toBe(true);
    expect(store.getState().surveyPrompt).toBeNull();
  });

  it('closeSurveyPanel hides the panel without submitting', () => {
    const store = makeStore();
    store.setState({ showSurveyPanel: true });
    store.getState().closeSurveyPanel();
    expect(store.getState().showSurveyPanel).toBe(false);
  });

  it('submitSurvey closes panel, shows thanks toast, and calls fetch', async () => {
    const store = makeStore();
    store.setState({ showSurveyPanel: true, sessionId: 'sess-123' });

    store.getState().submitSurvey({ experience: 'Good', feedback: 'Nice' });

    expect(store.getState().showSurveyPanel).toBe(false);
    expect(store.getState().transientAlert?.message).toBe(
      'Thanks for your feedback'
    );
    expect(store.getState().surveyState.lastCompletedAt).not.toBeNull();

    // Wait for the fire-and-forget fetch to complete
    await new Promise((r) => setTimeout(r, 50));

    // Verify fetch was called with the correct payload
    const fetchMock = globalThis.fetch as unknown as ReturnType<typeof mock>;
    expect(fetchMock).toHaveBeenCalled();
    const [, init] = (fetchMock as any).mock.calls[0] as [string, RequestInit];
    const body = JSON.parse(init.body as string);
    expect(body.category).toBe('KiroCLI');
    expect(body.name).toBe('SessionFeedback');
    expect(body.customerResponses[0].response.responseValue).toEqual(['4']);
    expect(
      body.metadataList.find((m: any) => m.key === 'sessionId')?.value
    ).toBe('sess-123');
  });

  it('submitSurvey resumes a queue blocked by the panel', async () => {
    const store = makeStore();
    const processQueue = mock(async () => {});
    store.setState({
      showSurveyPanel: true,
      queuedMessages: ['/tui'],
      processQueue,
    });

    store.getState().submitSurvey({ experience: 'Good' });
    await new Promise((resolve) => queueMicrotask(resolve));

    expect(processQueue).toHaveBeenCalledTimes(1);
  });

  it('dismissSurveyPrompt clears prompt bar and bumps dismiss count', () => {
    const store = makeStore();
    // Trigger notification
    store.getState().recordCompletedTurn();
    store.getState().recordCompletedTurn();
    store.getState().recordCompletedTurn();
    expect(store.getState().surveyPrompt).not.toBeNull();

    store.getState().dismissSurveyPrompt();
    expect(store.getState().surveyPrompt).toBeNull();
    expect(store.getState().surveyState.dismissCount).toBe(1);
  });

  it('does not show survey again within cooldown period', () => {
    const store = makeStore();
    // Trigger and dismiss
    store.getState().recordCompletedTurn();
    store.getState().recordCompletedTurn();
    store.getState().recordCompletedTurn();
    store.getState().dismissSurveyPrompt();

    // Set cooldown to 1 day so the dismiss we just did blocks re-trigger
    process.env.KIRO_SURVEY_COOLDOWN_DAYS = '1';

    // Try again — should not show because cooldown hasn't elapsed
    store.getState().recordCompletedTurn();
    expect(store.getState().transientAlert).toBeNull();
  });

  it('completed tasks do not block the survey', () => {
    const store = makeStore();
    store.setState({
      tasks: [{ id: '1', subject: 'Done', status: 'completed' }],
    });
    store.getState().recordCompletedTurn();
    store.getState().recordCompletedTurn();
    store.getState().recordCompletedTurn();
    // Completed tasks should NOT block — only pending ones do
    expect(store.getState().surveyPrompt).not.toBeNull();
  });

  it('typing a message while survey prompt is showing counts as dismissal toward cooldown', async () => {
    const store = makeStore();
    // Trigger the survey prompt
    store.getState().recordCompletedTurn();
    store.getState().recordCompletedTurn();
    store.getState().recordCompletedTurn();
    expect(store.getState().surveyPrompt).not.toBeNull();
    expect(store.getState().surveyState.dismissCount).toBe(0);

    // User types a message instead of pressing Ctrl+Y
    await store.getState().handleUserInput('hello');

    // Survey prompt should be cleared
    expect(store.getState().surveyPrompt).toBeNull();
    // Should count as a dismissal
    expect(store.getState().surveyState.dismissCount).toBe(1);
    // lastShownAt should be set (starts cooldown)
    expect(store.getState().surveyState.lastShownAt).not.toBeNull();
  });
});
