import { describe, it, expect, afterEach } from 'bun:test';
import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
} from '@agentclientprotocol/sdk';
import { requireChatCliBin } from '../src/utils/chat-cli-bin';
import { AcpTestCase } from './shared/AcpTestCase';
import { defaultKasModes } from './shared/default-agent';

const DOWN = '\x1b[B';

function setupHandshake(tc: AcpTestCase, sessionId = 'menu-layout-1'): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));
  tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
    sessionId,
    modes: defaultKasModes(),
  }));
  tc.mock.on('session/set_config_option', () => ({}));
}

/**
 * The picker option region: the rows between the `type to search` header
 * (plus its trailing blank line) and the footer divider that precedes the
 * `to navigate` hint line. Returns the slice of option/continuation rows.
 */
function optionRegion(snapshot: string[]): string[] {
  const searchIdx = snapshot.findIndex((l) => l.includes('type to search'));
  const footerIdx = snapshot.findIndex((l) => l.includes('to navigate'));
  if (searchIdx === -1 || footerIdx === -1) {
    throw new Error('could not locate picker region in snapshot');
  }
  // searchIdx + 1 is a blank spacer line; options start after it. The
  // footer divider sits one row above the hint line.
  return snapshot.slice(searchIdx + 2, footerIdx - 1);
}

function blankRows(rows: string[]): number {
  return rows.filter((l) => l.trim() === '').length;
}

describe('/chat picker layout', () => {
  let tc: AcpTestCase | null = null;
  let kiroHome: string | null = null;
  const REAL_BIN = requireChatCliBin();

  function makeCase(width = 48, name = 'chat-picker-layout'): AcpTestCase {
    kiroHome = realpathSync(mkdtempSync(join(tmpdir(), 'kiro-chat-menu-')));
    const now = Date.now();
    const c = new AcpTestCase({
      testName: name,
      cwd: kiroHome,
      terminalSize: { width, height: 24 },
      mockKasSessionListResult: [
        {
          sessionId: 'sess-long-0001',
          cwd: kiroHome,
          title:
            'A very long session title that should exceed the row width and wrap onto multiple lines',
          updatedAt: new Date(now - 60_000).toISOString(),
        },
        {
          sessionId: 'sess-short-002',
          cwd: kiroHome,
          title: 'short one',
          updatedAt: new Date(now - 120_000).toISOString(),
        },
        {
          sessionId: 'sess-long-0003',
          cwd: kiroHome,
          title:
            'Another lengthy title describing a refactor of the dispatcher and command menu',
          updatedAt: new Date(now - 180_000).toISOString(),
        },
        {
          sessionId: 'sess-short-004',
          cwd: kiroHome,
          title: 'tiny',
          updatedAt: new Date(now - 240_000).toISOString(),
        },
      ],
      extraEnv: { KIRO_CHAT_CLI_BIN: REAL_BIN, KIRO_HOME: kiroHome },
    });
    setupHandshake(c);
    return c;
  }

  async function openPicker(c: AcpTestCase): Promise<void> {
    await c.launch();
    await c.mock.awaitConnection();
    await c.waitForVisibleText('ask a question', 10000);
    await c.sleepMs(300);
    await c.sendKeys('/chat');
    await c.sleepMs(300);
    await c.waitForVisibleText('/chat', 5000);
    await c.sendKeys('\r');
    await c.waitForVisibleText('short one', 5000);
    await c.sleepMs(200);
  }

  afterEach(async () => {
    if (tc) {
      await tc.cleanup();
      tc = null;
    }
    if (kiroHome) {
      rmSync(kiroHome, { recursive: true, force: true });
      kiroHome = null;
    }
  });

  it('a highlighted multi-line item keeps its caret', async () => {
    tc = makeCase(48, 'chat-picker-caret');
    await openPicker(tc);

    // The most-recent (long, multi-line) session is highlighted by
    // default at index 0. A highlighted item must show its `❯` caret.
    const caret = tc.findTextCells('❯');
    expect(caret).not.toBeNull();
  }, 40000);

  it('highlighting a short item does not change its height', async () => {
    tc = makeCase(48, 'chat-picker-height');
    await openPicker(tc);

    // Baseline: with the long item highlighted, the option region wraps
    // only real title text - no blank filler rows.
    const initialBlanks = blankRows(optionRegion(tc.getSnapshot()));
    expect(initialBlanks).toBe(0);

    // Move highlight onto the short single-line item. Selection must not
    // introduce blank rows: its height should stay at one physical row.
    await tc.sendKeys(DOWN);
    await tc.sleepMs(200);

    const hoveredBlanks = blankRows(optionRegion(tc.getSnapshot()));
    expect(hoveredBlanks).toBe(0);
  }, 40000);

  it('keeps the relative-time description visible next to a long label', async () => {
    tc = makeCase(72, 'chat-picker-timestamp');
    await openPicker(tc);

    // Even though the first item is a long, wrapping title, the timestamp
    // column must not be squeezed off the row. The reserve is small, so the
    // timestamp may be truncated ("3 days...") - assert the surviving
    // leading fragment (number + unit) rather than the full string.
    const region = optionRegion(tc.getSnapshot());
    const hasTimestamp = region.some((l) => /\d+\s+(sec|min|hou|day)/.test(l));
    expect(hasTimestamp).toBe(true);
  }, 40000);
});
