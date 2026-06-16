/**
 * ACP wire-level tests for the thinking display (<ThinkingDisplay>):
 *   - reasoning text is stored on the model message with a duration
 *   - collapsed by default: header + hint, body hidden
 *   - ctrl+o expands/collapses the reasoning stream
 *   - once flushed to scrollback it shows a frozen "Thought for Ns" hint
 *   - cancelling mid-reasoning still finalizes the block (no stuck "Thinking...")
 *
 * `chat.showThinking` is set per-test via the sandbox settings so each case is
 * resilient to default changes.
 */
import { describe, it, expect, afterEach } from 'bun:test';
import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
  PromptRequest,
  PromptResponse,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from './shared/AcpTestCase';
import { defaultKasModes } from './shared/default-agent';

const SESSION_ID = 'thinking-display-session';

const REASONING = Array.from(
  { length: 6 },
  (_, i) => `Reasoning step ${i + 1}.`
).join('\n');

function setupHandshake(tc: AcpTestCase): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));
  tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
    sessionId: SESSION_ID,
    modes: defaultKasModes(),
  }));
  tc.mock.on('session/set_config_option', () => ({}));
}

function thought(tc: AcpTestCase, text: string): void {
  tc.mock.notify('session/update', {
    sessionId: SESSION_ID,
    update: {
      sessionUpdate: 'agent_thought_chunk',
      content: { type: 'text', text },
    },
  });
}

function message(tc: AcpTestCase, text: string): void {
  tc.mock.notify('session/update', {
    sessionId: SESSION_ID,
    update: {
      sessionUpdate: 'agent_message_chunk',
      content: { type: 'text', text },
    },
  });
}

describe('Thinking display', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('stores thinking text and duration on the model message', async () => {
    tc = new AcpTestCase({
      testName: 'thinking-store',
      settings: { 'chat.showThinking': 'collapsed' },
    });
    setupHandshake(tc);
    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      thought(tc!, 'Let me reason about this.');
      message(tc!, 'Here is my answer.');
      return { stopReason: 'end_turn' };
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);

    await tc.sendKeys('hello');
    await tc.pressEnter();
    await tc.waitForVisibleText('Here is my answer', 10000);

    const state = await tc.getStore();
    const modelMsg = state.messages.find((m) => m.role === 'model') as
      | Record<string, unknown>
      | undefined;
    expect(modelMsg).toBeDefined();
    expect(modelMsg!.content).toContain('Here is my answer.');
    expect(modelMsg!.thinking).toBe('Let me reason about this.');
    expect(typeof modelMsg!.thinkingMs).toBe('number');
  }, 30000);

  it('collapses by default: header + hint shown, reasoning body hidden', async () => {
    tc = new AcpTestCase({
      testName: 'thinking-collapsed',
      settings: { 'chat.showThinking': 'collapsed' },
    });
    setupHandshake(tc);
    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      thought(tc!, REASONING);
      message(tc!, 'All done.');
      return { stopReason: 'end_turn' };
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);

    await tc.sendKeys('hello');
    await tc.pressEnter();
    await tc.waitForVisibleText('All done.', 10000);
    await tc.sleepMs(300);

    const snap = tc.getSnapshot().join('\n');
    expect(snap).toContain('Thought for');
    expect(snap).toContain('ctrl+o to view');
    expect(snap).not.toContain('Reasoning step 1.');
    expect(snap).not.toContain('Reasoning step 6.');
  }, 30000);

  it('ctrl+o expands the full stream, then collapses it again', async () => {
    tc = new AcpTestCase({
      testName: 'thinking-ctrl-o',
      settings: { 'chat.showThinking': 'collapsed' },
    });
    setupHandshake(tc);
    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      thought(tc!, REASONING);
      message(tc!, 'All done.');
      return { stopReason: 'end_turn' };
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);

    await tc.sendKeys('hello');
    await tc.pressEnter();
    await tc.waitForVisibleText('All done.', 10000);
    await tc.sleepMs(300);

    let snap = tc.getSnapshot().join('\n');
    expect(snap).not.toContain('Reasoning step 1.');
    expect(snap).toContain('ctrl+o to view');

    // ctrl+o → expanded: every line visible, hint flips to collapse.
    await tc.sendKeys([0x0f]);
    await tc.sleepMs(300);
    snap = tc.getSnapshot().join('\n');
    for (let i = 1; i <= 6; i++) {
      expect(snap).toContain(`Reasoning step ${i}.`);
    }
    expect(snap).toContain('ctrl+o to collapse details');

    // ctrl+o again → collapsed: body hidden, hint flips back.
    await tc.sendKeys([0x0f]);
    await tc.sleepMs(300);
    snap = tc.getSnapshot().join('\n');
    expect(snap).not.toContain('Reasoning step 1.');
    expect(snap).not.toContain('Reasoning step 6.');
    expect(snap).toContain('ctrl+o to view');
  }, 30000);

  it('static (history) buffer shows only the "Thought for Ns" hint, frozen', async () => {
    tc = new AcpTestCase({
      testName: 'thinking-static',
      settings: { 'chat.showThinking': 'collapsed' },
    });
    setupHandshake(tc);
    let promptCount = 0;
    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      promptCount += 1;
      if (promptCount === 1) {
        thought(tc!, REASONING);
        message(tc!, 'First answer.');
      } else {
        message(tc!, 'Second answer.');
      }
      return { stopReason: 'end_turn' };
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);

    await tc.sendKeys('hello');
    await tc.pressEnter();
    await tc.waitForVisibleText('First answer.', 10000);

    // A second prompt completes turn 1, flushing it to <Static>.
    await tc.sendKeys('again');
    await tc.pressEnter();
    await tc.waitForVisibleText('Second answer.', 10000);
    await tc.sleepMs(300);

    const snap = tc.getSnapshot().join('\n');
    expect(snap).toContain('Thought for');
    expect(snap).not.toContain('Reasoning step 1.');
    expect(snap).not.toContain('Reasoning step 6.');
    expect(snap).not.toContain('ctrl+o to view');
    expect(snap).not.toContain('ctrl+o to collapse details');
  }, 30000);

  it('finalizes the reasoning block when cancelled mid-reasoning', async () => {
    // Reasoning streamed, turn held open; Ctrl+C must close the block
    // ("Thought for Ns") instead of leaving a stuck "Thinking..." header.
    tc = new AcpTestCase({
      testName: 'thinking-cancel',
      settings: { 'chat.showThinking': 'expanded' },
    });
    setupHandshake(tc);
    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      thought(tc!, 'I am reasoning about the request right now.');
      await new Promise((r) => setTimeout(r, 15000));
      return { stopReason: 'end_turn' };
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);

    await tc.sendKeys('think then stop');
    await tc.pressEnter();
    await tc.waitForStore(
      (s) =>
        s.isProcessing &&
        s.messages.some(
          (m) => m.role === 'model' && !!(m as Record<string, unknown>).thinking
        ),
      10000
    );

    await tc.pressCtrlC();

    const state = await tc.waitForStore(
      (s) =>
        !s.isProcessing &&
        s.messages.some(
          (m) =>
            m.role === 'model' &&
            !!(m as Record<string, unknown>).thinking &&
            typeof (m as Record<string, unknown>).thinkingMs === 'number'
        ),
      10000
    );

    const modelMsg = state.messages.find(
      (m) => m.role === 'model' && !!(m as Record<string, unknown>).thinking
    ) as Record<string, unknown> | undefined;
    expect(modelMsg).toBeDefined();
    expect((modelMsg!.thinkingMs as number) > 0).toBe(true);

    await tc.waitForVisibleText('Thought for', 5000);
  }, 40000);
});
