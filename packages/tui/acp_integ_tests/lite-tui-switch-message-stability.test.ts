import { afterEach, describe, expect, it } from 'bun:test';
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
import { CMD_TUI } from '../integ_tests/helpers/commands';
import { visibleCount, visibleIndex } from '../integ_tests/helpers/mode-swap';

const SESSION_ID = 'kas-lite-tui-stability-session';

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

async function typeSlashCommand(
  tc: AcpTestCase,
  command: string
): Promise<void> {
  for (const char of command) {
    await tc.sendKeys(char);
    await tc.sleepMs(30);
  }
  await tc.sleepMs(200);
  await tc.pressEnter();
}

function notifyText(tc: AcpTestCase, text: string): void {
  tc.mock.notify('session/update', {
    sessionId: SESSION_ID,
    update: {
      sessionUpdate: 'agent_message_chunk',
      content: { type: 'text', text },
    },
  });
}

function notifyReadTool(tc: AcpTestCase): void {
  tc.mock.notify('session/update', {
    sessionId: SESSION_ID,
    update: {
      sessionUpdate: 'tool_call',
      toolCallId: 'post-switch-read',
      title: 'read_file',
      kind: 'read',
      rawInput: { path: 'package.json' },
    },
  });
  tc.mock.notify('session/update', {
    sessionId: SESSION_ID,
    update: {
      sessionUpdate: 'tool_call_update',
      toolCallId: 'post-switch-read',
      status: 'completed',
      rawOutput: { response: '{"contents":"{}"}' },
    },
  });
}

describe('KAS lite→TUI switch message stability', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('keeps completed streamed TUI turns visible and ordered after flushing', async () => {
    tc = new AcpTestCase({
      testName: 'kas-lite-tui-completion-stability',
      terminalSize: { width: 120, height: 60 },
      extraEnv: {
        KIRO_UI_MODE: 'lite',
        KIRO_LITE_ROLLOUT_ENABLED: '1',
      },
    });
    setupHandshake(tc);

    let promptCount = 0;
    tc.mock.on<PromptRequest, PromptResponse>('session/prompt', async () => {
      promptCount += 1;
      if (promptCount === 1) {
        notifyText(tc!, 'LITE_RESPONSE_MARKER');
      } else if (promptCount === 2) {
        notifyText(tc!, 'POST_SWITCH_STREAMED_TEXT');
        await tc!.sleepMs(100);
        notifyReadTool(tc!);
        await tc!.sleepMs(100);
        notifyText(tc!, 'POST_SWITCH_FINAL_TEXT');
      } else if (promptCount === 3) {
        notifyText(tc!, 'POST_SWITCH_SECOND_TURN');
      }
      return { stopReason: 'end_turn' };
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('>', 10000);

    let store = await tc.getStore();
    expect(store.agentEngine).toBe('kas');
    expect(store.uiMode).toBe('lite');

    await tc.sendKeys('hello');
    await tc.sleepMs(100);
    await tc.pressEnter();
    await tc.waitForVisibleText('LITE_RESPONSE_MARKER', 10000);
    await tc.waitForStore((s) => !s.isProcessing, 10000);

    await typeSlashCommand(tc, CMD_TUI);
    await tc.waitForStore((s) => s.uiMode === 'tui', 10000);
    await tc.waitForVisibleText('Switched to TUI mode', 10000);

    await tc.sendKeys('tui msg');
    await tc.sleepMs(100);
    await tc.pressEnter();
    await tc.waitForVisibleText('POST_SWITCH_FINAL_TEXT', 10000);
    await tc.waitForStore((s) => !s.isProcessing, 10000);

    await tc.sendKeys('flush prior tui turn');
    await tc.sleepMs(100);
    await tc.pressEnter();
    await tc.waitForVisibleText('POST_SWITCH_SECOND_TURN', 10000);
    await tc.waitForStore((s) => !s.isProcessing, 10000);

    store = await tc.getStore();
    expect(store.uiMode).toBe('tui');
    expect(promptCount).toBe(3);

    const snap = tc.getSnapshot();
    const allText = snap.join('\n');
    expect(allText).toContain('POST_SWITCH_STREAMED_TEXT');
    expect(allText).toContain('package.json');
    expect(allText).toContain('POST_SWITCH_FINAL_TEXT');
    expect(allText).toContain('POST_SWITCH_SECOND_TURN');
    expect(visibleCount(snap, 'Switched to TUI mode')).toBe(1);
    expect(visibleCount(snap, 'LITE_RESPONSE_MARKER')).toBe(1);

    const liteIdx = visibleIndex(snap, 'LITE_RESPONSE_MARKER');
    const switchIdx = visibleIndex(snap, 'Switched to TUI mode');
    const streamedIdx = visibleIndex(snap, 'POST_SWITCH_STREAMED_TEXT');
    const toolIdx = visibleIndex(snap, 'package.json');
    const finalIdx = visibleIndex(snap, 'POST_SWITCH_FINAL_TEXT');
    const secondIdx = visibleIndex(snap, 'POST_SWITCH_SECOND_TURN');
    expect(liteIdx).toBeGreaterThanOrEqual(0);
    expect(switchIdx).toBeGreaterThanOrEqual(0);
    expect(streamedIdx).toBeGreaterThan(switchIdx);
    expect(toolIdx).toBeGreaterThan(streamedIdx);
    expect(finalIdx).toBeGreaterThan(toolIdx);
    expect(secondIdx).toBeGreaterThan(finalIdx);
  }, 60000);
});
