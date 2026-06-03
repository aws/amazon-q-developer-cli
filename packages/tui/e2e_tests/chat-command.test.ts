import { afterEach, describe, expect, it } from 'bun:test';
import * as fs from 'node:fs';
import * as path from 'node:path';
import { E2ETestCase } from './E2ETestCase';

describe('Chat Command', () => {
  let testCase: E2ETestCase | null = null;

  afterEach(async () => {
    if (testCase) {
      await testCase.cleanup();
      testCase = null;
    }
  });

  it.skipIf(process.platform === 'win32')(
    'loads a previous session and displays its history',
    async () => {
      testCase = await E2ETestCase.builder()
        .withTerminal({ width: 120, height: 40 })
        .withTestName('chat-command-load')
        .withGlobalAgentConfig('test-agent', {
          name: 'test-agent',
          description: 'Test agent for e2e',
          tools: ['@builtin'],
        })
        .launch();

      // Create a session with history via a separate ACP connection
      const acp = await testCase.launchAcpHelper();
      const sessionId = await acp.newSession();

      // Switch to custom agent
      await acp.setSessionMode(sessionId, 'test-agent');

      await acp.pushResponse(sessionId, [
        {
          kind: 'event',
          data: {
            kind: 'AssistantResponseEvent',
            data: { content: 'The answer is 4.' },
          },
        },
      ]);
      await acp.pushResponse(sessionId, null);
      await acp.prompt(sessionId, 'What is 2+2?');
      await acp.terminateSession(sessionId);
      await acp.close();

      // Load the session via /chat in the TUI
      await testCase.waitForText('ask a question', 15000);
      await testCase.waitForSlashCommands(15000);

      for (const char of '/chat') {
        await testCase.sendKeys(char);
        await testCase.sleepMs(50);
      }
      await testCase.pressEnter();

      await testCase.waitForText('What is 2+2?', 10000);
      await testCase.pressEnter();

      await testCase.sleepMs(2000);

      try {
        await testCase.waitForText('What is 2+2?', 60000);
        await testCase.waitForText('The answer is 4.', 5000);
      } catch (e) {
        console.log('FAILED snapshot:\n' + testCase.getSnapshotFormatted());
        const store = await testCase.getStore();
        console.log('Store messages:', JSON.stringify(store.messages, null, 2));
        console.log('Store sessionId:', store.sessionId);
        throw e;
      }

      const store = await testCase.getStore();
      expect(
        store.messages.some((m) => m.content.includes('What is 2+2?'))
      ).toBe(true);
      expect(
        store.messages.some((m) => m.content.includes('The answer is 4.'))
      ).toBe(true);
      // Verify system delimiter message was added
      expect(
        store.messages.some(
          (m) => m.role === 'system' && m.content.includes('Loaded session')
        )
      ).toBe(true);
      // Verify session ID was updated
      expect(store.sessionId).toBe(sessionId);
      // Verify agent is set (persisted from session creation)
      expect(store.currentAgent).not.toBeNull();
      expect(store.currentAgent!.name).toBe('test-agent');
    },
    120000
  );

  it.skipIf(process.platform === 'win32')(
    'replays persisted thinking blocks on resume',
    async () => {
      testCase = await E2ETestCase.builder()
        .withTerminal({ width: 120, height: 40 })
        .withTestName('chat-cmd-resume-think')
        // Explicitly set `chat.showThinking` to true to be resilient against
        // future default changes. The setting itself is exercised by
        // show-thinking-setting.test.ts.
        .withGlobalSettings({ 'chat.showThinking': true })
        .launch();

      // Create a session whose persisted log includes a reasoning block
      // followed by a regular assistant response.
      const acp = await testCase.launchAcpHelper();
      const sessionId = await acp.newSession();

      await acp.pushResponse(sessionId, [
        {
          kind: 'event',
          data: {
            kind: 'ReasoningEvent',
            data: {
              text:
                'Thinking step one.\n' +
                'Thinking step two.\n' +
                'Thinking step three.',
            },
          },
        },
        {
          kind: 'event',
          data: {
            kind: 'ReasoningEvent',
            data: { signature: 'test-sig' },
          },
        },
        {
          kind: 'event',
          data: {
            kind: 'AssistantResponseEvent',
            data: { content: 'The answer is 4.' },
          },
        },
      ]);
      await acp.pushResponse(sessionId, null);
      await acp.prompt(sessionId, 'What is 2+2?');
      await acp.terminateSession(sessionId);
      await acp.close();

      // Load the session via /chat in the TUI.
      await testCase.waitForText('ask a question', 15000);
      await testCase.waitForSlashCommands(15000);

      for (const char of '/chat') {
        await testCase.sendKeys(char);
        await testCase.sleepMs(50);
      }
      await testCase.pressEnter();

      await testCase.waitForText('What is 2+2?', 10000);
      await testCase.pressEnter();

      await testCase.sleepMs(2000);

      try {
        await testCase.waitForText('What is 2+2?', 60000);
        await testCase.waitForText('The answer is 4.', 10000);
        // The ThinkingDisplay renders a "● Thinking" header and (for >4 lines)
        // a tail of the reasoning. With 3 short lines they should all be
        // visible in the static snapshot, with a "Thinking" header above.
        await testCase.waitForText('Thinking', 5000);
        await testCase.waitForText('Thinking step three.', 5000);
      } catch (e) {
        console.log('FAILED snapshot:\n' + testCase.getSnapshotFormatted());
        const store = await testCase.getStore();
        console.log('Store messages:', JSON.stringify(store.messages, null, 2));
        throw e;
      }

      const snapshot = testCase.getSnapshot().join('\n');
      expect(snapshot).toContain('Thinking step one.');
      expect(snapshot).toContain('Thinking step three.');

      // The Model message should have its `thinking` field populated by the
      // replayed AgentThoughtChunk. (Empty/missing would imply the chunk was
      // dropped on the way through `log_entry_to_session_updates`.)
      const store = await testCase.getStore();
      const modelMsg = store.messages.find(
        (m): m is typeof m & { role: 'model' } =>
          m.role === 'model' && m.content.includes('The answer is 4.')
      );
      expect(modelMsg).toBeTruthy();
      const thinking = (modelMsg as any)?.thinking;
      expect(thinking).toBeTruthy();
      expect(thinking).toContain('Thinking step one.');
    },
    120000
  );

  it('/chat new starts a fresh conversation', async () => {
    testCase = await E2ETestCase.builder()
      .withTerminal({ width: 120, height: 40 })
      .withTestName('chat-new')
      .launch();

    await testCase.waitForText('ask a question', 15000);
    await testCase.waitForSlashCommands(15000);

    const initialSessionId = await testCase.getSessionId();

    await testCase.pushSendMessageResponse([
      {
        kind: 'event',
        data: {
          kind: 'AssistantResponseEvent',
          data: { content: 'Hello there!' },
        },
      },
    ]);
    await testCase.pushSendMessageResponse(null);

    for (const char of 'hi') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(50);
    }
    await testCase.pressEnter();
    await testCase.waitForText('Hello there!', 30000);
    await testCase.waitForIdle();

    for (const char of '/chat new') {
      await testCase.sendKeys(char);
      await testCase.sleepMs(50);
    }
    await testCase.pressEnter();

    const deadline = Date.now() + 15000;
    let store = await testCase.getStore();
    while (store.sessionId === initialSessionId && Date.now() < deadline) {
      await testCase.sleepMs(200);
      store = await testCase.getStore();
    }

    expect(store.sessionId).not.toBe(initialSessionId);
    expect(store.sessionId).toBeTruthy();

    const hasOldContent = store.messages.some((m) =>
      m.content.includes('Hello there!')
    );
    expect(hasOldContent).toBe(false);
  }, 120000);

  it.skipIf(process.platform === 'win32')(
    '/chat save followed by /chat load creates a fresh session id on disk',
    async () => {
      testCase = await E2ETestCase.builder()
        .withTerminal({ width: 120, height: 40 })
        .withTestName('chat-save-load-roundtrip')
        .launch();

      await testCase.waitForText('ask a question', 15000);
      await testCase.waitForSlashCommands(15000);

      const originalSessionId = await testCase.getSessionId();

      // Populate the session so the V2 backend's save_session has
      // metadata + log on disk to read.
      await testCase.pushSendMessageResponse([
        {
          kind: 'event',
          data: {
            kind: 'AssistantResponseEvent',
            data: { content: 'Hello from mock!' },
          },
        },
      ]);
      await testCase.pushSendMessageResponse(null);

      for (const char of 'hi') {
        await testCase.sendKeys(char);
        await testCase.sleepMs(50);
      }
      await testCase.pressEnter();
      await testCase.waitForText('Hello from mock!', 30000);
      await testCase.waitForIdle();

      // E2ETestCase points KIRO_TEST_SESSIONS_DIR at sandboxDir, so V2
      // session files land directly under sandboxDir as `<id>.json` /
      // `<id>.jsonl`.
      const originalJsonPath = path.join(
        testCase.sandboxDir,
        `${originalSessionId}.json`
      );
      expect(fs.existsSync(originalJsonPath)).toBe(true);

      // Save the session to a temp file outside the sessions area.
      const exportPath = path.join(testCase.sandboxDir, 'export.json');
      for (const char of `/chat save ${exportPath}`) {
        await testCase.sendKeys(char);
        await testCase.sleepMs(20);
      }
      await testCase.pressEnter();
      await testCase.waitForText(`Saved session to ${exportPath}`, 15000);
      expect(fs.existsSync(exportPath)).toBe(true);

      // Load the saved file. The V2 backend's load_session generates a
      // fresh UUID and writes it under the sessions dir, then the TUI
      // session/loads it.
      for (const char of `/chat load ${exportPath}`) {
        await testCase.sendKeys(char);
        await testCase.sleepMs(20);
      }
      await testCase.pressEnter();
      await testCase.waitForText('Session loaded', 15000);

      // The loaded session's id is reflected in the store.
      const deadline = Date.now() + 10000;
      let store = await testCase.getStore();
      while (store.sessionId === originalSessionId && Date.now() < deadline) {
        await testCase.sleepMs(200);
        store = await testCase.getStore();
      }
      const newSessionId = store.sessionId;
      expect(newSessionId).not.toBe(originalSessionId);
      expect(newSessionId).toBeTruthy();

      const newJsonPath = path.join(
        testCase.sandboxDir,
        `${newSessionId}.json`
      );
      expect(fs.existsSync(newJsonPath)).toBe(true);

      // Original session was preserved - load doesn't replace it.
      expect(fs.existsSync(originalJsonPath)).toBe(true);
    },
    120000
  );
});
