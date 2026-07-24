/**
 * Integration tests for the /spec new description-collection step.
 *
 * Covers what unit tests can't: the real PTY rendering of the live-region
 * intro (appears on arm, vanishes on submit/cancel with no transcript
 * trace), the placeholder hint, and esc ownership between the prompt input
 * and an open panel.
 *
 * The mock session doesn't advertise commands on boot, so each test seeds
 * the KAS command registry by injecting KasCommandsDiscovered and driving
 * one priming turn (queued events drain when a prompt starts).
 */
import { afterEach, describe, expect, it } from 'bun:test';
import { TestCase } from '../src/test-utils/TestCase';
import { AgentEventType, ContentType } from '../src/types/agent-events';
import { KAS_COMMANDS } from '../src/kas-commands';

const INTRO = 'Starting spec: "web-clock"';
const PLACEHOLDER = 'describe what "web-clock" should do';

async function launchWithSpecCommand(testName: string): Promise<TestCase> {
  const testCase = await TestCase.builder()
    .withTestName(testName)
    .withTimeout(15000)
    .launch();
  await testCase.waitForVisibleText('ask a question', 15000);

  // Queued until the priming turn below starts, then registers /spec.
  await testCase.mockSessionUpdate({
    type: AgentEventType.KasCommandsDiscovered,
    commands: [...KAS_COMMANDS],
  });
  await testCase.mockSessionUpdate({
    type: AgentEventType.Content,
    id: 'prime-msg',
    content: { type: ContentType.Text, text: 'primed' },
  });
  await testCase.typeAndSubmit('hi');
  await testCase.sleepMs(200);
  await testCase.completeTurn();
  await testCase.sleepMs(300);
  return testCase;
}

async function armDescriptionStep(testCase: TestCase): Promise<void> {
  await testCase.typeAndSubmit('/spec new web-clock');
  await testCase.sleepMs(400);
  const store = await testCase.getStore();
  expect(store.pendingSpecDescription?.featureName).toBe('web-clock');
}

describe('/spec new description collection', () => {
  let testCase: TestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it('arming shows the intro and placeholder without touching the transcript', async () => {
    testCase = await launchWithSpecCommand('spec-desc-arm');
    const messagesBefore = (await testCase.getStore()).messages.length;

    await armDescriptionStep(testCase);

    const snap = testCase.getSnapshot().join('\n');
    expect(snap).toContain(INTRO);
    expect(snap).toContain('What should this spec cover?');
    expect(snap).toContain(PLACEHOLDER);

    const store = await testCase.getStore();
    expect(store.currentAgent?.name).toBe('spec');
    // Live-region only: no message was written for the intro.
    expect(store.messages.length).toBe(messagesBefore);
  }, 30000);

  it('submitting a description sends it and dismisses the intro', async () => {
    testCase = await launchWithSpecCommand('spec-desc-submit');
    await armDescriptionStep(testCase);

    await testCase.typeAndSubmit('A clock that counts up and down');
    await testCase.sleepMs(300);
    await testCase.completeTurn();
    await testCase.sleepMs(300);

    const store = await testCase.getStore();
    expect(store.pendingSpecDescription).toBeNull();
    // The transcript shows the typed description, not the kickoff prompt.
    const userMessages = store.messages.filter((m) => m.role === 'user');
    const lastUser = userMessages[userMessages.length - 1] as {
      content: string;
    };
    expect(lastUser.content).toContain('A clock that counts up and down');
    expect(lastUser.content).not.toContain('Start a new spec');

    const snap = testCase.getSnapshot().join('\n');
    expect(snap).not.toContain(PLACEHOLDER);
  }, 30000);

  it('esc cancels tracelessly and stays in spec mode', async () => {
    testCase = await launchWithSpecCommand('spec-desc-esc');
    const messagesBefore = (await testCase.getStore()).messages.length;
    await armDescriptionStep(testCase);

    await testCase.pressEscape();
    await testCase.sleepMs(400);

    const store = await testCase.getStore();
    expect(store.pendingSpecDescription).toBeNull();
    // Mode is untouched — leaving spec is an explicit action.
    expect(store.currentAgent?.name).toBe('spec');
    // Traceless: no message added, intro gone from the screen.
    expect(store.messages.length).toBe(messagesBefore);
    const snap = testCase.getSnapshot().join('\n');
    expect(snap).not.toContain(INTRO);
    expect(snap).toContain('Spec setup cancelled');
  }, 30000);

  it('a panel command runs without ending the step, and its esc only closes the panel', async () => {
    testCase = await launchWithSpecCommand('spec-desc-panel-esc');
    await armDescriptionStep(testCase);

    await testCase.typeAndSubmit('/tui');
    await testCase.sleepMs(400);

    // The command runs and the step survives it.
    let store = await testCase.getStore();
    expect(store.showTuiPanel).toBe(true);
    expect(store.pendingSpecDescription?.featureName).toBe('web-clock');

    // Esc belongs to the panel here — the step must not be cancelled too.
    await testCase.pressEscape();
    await testCase.sleepMs(400);

    store = await testCase.getStore();
    expect(store.showTuiPanel).toBe(false);
    expect(store.pendingSpecDescription?.featureName).toBe('web-clock');
    expect(store.currentAgent?.name).toBe('spec');
    expect(testCase.getSnapshot().join('\n')).toContain(PLACEHOLDER);
  }, 30000);

  it('esc with the slash menu open closes the menu; only a bare esc cancels', async () => {
    testCase = await launchWithSpecCommand('spec-desc-menu-esc');
    await armDescriptionStep(testCase);

    // "/" opens the command dropdown mid-step.
    await testCase.sendKeys('/');
    await testCase.sleepMs(300);
    let store = await testCase.getStore();
    expect(store.activeTrigger?.key).toBe('/');

    // First esc: the menu owns it — the step must survive.
    await testCase.pressEscape();
    await testCase.sleepMs(300);
    store = await testCase.getStore();
    expect(store.activeTrigger).toBeNull();
    expect(store.pendingSpecDescription?.featureName).toBe('web-clock');

    // Second (bare) esc cancels the step.
    await testCase.pressEscape();
    await testCase.sleepMs(300);
    store = await testCase.getStore();
    expect(store.pendingSpecDescription).toBeNull();
    expect(store.currentAgent?.name).toBe('spec');
  }, 30000);
});
