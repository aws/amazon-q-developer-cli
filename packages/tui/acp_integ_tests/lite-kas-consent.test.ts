/**
 * ACP integ test: KAS granular permission consent through the LITE UI.
 *
 * permission-consent.test.ts already covers this wire surface for the full TUI;
 * lite has its own ApprovalPrompt with a distinct keyboard model ([t] opens the
 * scope page, [s] cycles persistence scope, ↑↓ + enter select a row) that the
 * real KasAcpClient must still translate into the same
 * `session/request_permission` response `_meta.kiro.consent`. This runs the real
 * KasAcpClient against the mock ACP wire with KIRO_UI_MODE=lite so the lite
 * prompt → store (buildKasConsentMeta) → wire path is exercised end to end.
 */
import { describe, it, expect, afterEach } from 'bun:test';
import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from '../src/test-utils/acp-mock/AcpTestCase';
import { defaultKasModes } from './shared/default-agent';

const SESSION_ID = 'lite-consent-session';
const COMPOUND_SHELL_COMMAND = 'git status && echo "done"';
const GATED_SHELL_SEGMENT = 'echo "done"';

function liteKas(testName: string): AcpTestCase {
  return new AcpTestCase({
    testName,
    extraEnv: { KIRO_UI_MODE: 'lite', KIRO_LITE_ROLLOUT_ENABLED: '1' },
  });
}

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
  tc.mock.on('session/prompt', () => ({ stopReason: 'end_turn' }));
}

function sendShellToolCall(tc: AcpTestCase, toolCallId: string): void {
  tc.mock.notify('session/update', {
    sessionId: SESSION_ID,
    update: {
      sessionUpdate: 'tool_call',
      toolCallId,
      title: 'Run Command',
      kind: 'execute',
      rawInput: { command: COMPOUND_SHELL_COMMAND },
    },
  });
}

function requestShellPermission(tc: AcpTestCase, toolCallId: string) {
  return tc.mock.request('session/request_permission', {
    sessionId: SESSION_ID,
    toolCall: { toolCallId },
    options: [
      { kind: 'allow_once', name: 'Allow Once', optionId: 'accept' },
      { kind: 'allow_always', name: 'Always Allow', optionId: 'always-accept' },
      { kind: 'reject_once', name: 'Reject Once', optionId: 'reject' },
    ],
    _meta: {
      kiro: {
        toolId: 'shell-tool',
        consent: {
          capability: 'shell',
          resource: COMPOUND_SHELL_COMMAND,
          triggeringResource: GATED_SHELL_SEGMENT,
        },
      },
    },
  });
}

/** Boot lite+KAS, push the shell tool + permission request, wait for the prompt. */
async function openShellApproval(
  tc: AcpTestCase,
  toolCallId: string
): Promise<{ responsePromise: Promise<any> }> {
  setupHandshake(tc);
  await tc.launch();
  await tc.mock.awaitConnection();
  await tc.waitForVisibleText('ask a question', 10000);

  const store = await tc.getStore();
  expect(store.agentEngine).toBe('kas');
  expect(store.uiMode).toBe('lite');

  sendShellToolCall(tc, toolCallId);
  await tc.sleepMs(200);
  const responsePromise = requestShellPermission(tc, toolCallId);

  await tc.waitForStore((s) => s.pendingApproval !== null, 5000);
  // The prompt mounts (and its key handler arms) only once the APPROVAL_IDLE_MS
  // debounce elapses — LiteLayout gates the whole component on `approvalReady`.
  // Waiting on the painted hotkey row is the debounce barrier; a fixed sleep
  // here would race the 2s timer. Budget covers debounce + paint.
  await tc.waitForVisibleText('[t] trust scope', 6000);
  // [t] opens the kas-scope page; "trust scope [session]" is the page header
  // (distinct from the default row's "[t] trust scope" hotkey label).
  await tc.sendKeys('t');
  await tc.waitForVisibleText('trust scope [session]', 3000);
  return { responsePromise: responsePromise as Promise<any> };
}

describe('KAS lite consent flow (wire)', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('allow once → response scope is "invocation"', async () => {
    tc = liteKas('lite-consent-allow-once');
    setupHandshake(tc);
    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);

    sendShellToolCall(tc, 'tc-allow-once');
    await tc.sleepMs(200);
    const responsePromise = requestShellPermission(tc, 'tc-allow-once');

    await tc.waitForStore((s) => s.pendingApproval !== null, 5000);
    // The prompt arms only after the APPROVAL_IDLE_MS debounce; wait on the
    // painted hotkey row rather than a fixed sleep that races the timer.
    await tc.waitForVisibleText('[y] allow once', 6000);
    // [y] is the lite allow-once hotkey (no menu navigation).
    await tc.sendKeys('y');

    const response = (await responsePromise) as any;
    expect(response.outcome.outcome).toBe('selected');
    expect(response.outcome.optionId).toBe('accept');
    expect(response._meta?.kiro?.consent?.capability).toBe('shell');
    expect(response._meta?.kiro?.consent?.scope).toBe('invocation');
  }, 30000);

  it('[t] → exact row trusts the GATED segment at session scope', async () => {
    tc = liteKas('lite-consent-exact');
    const { responsePromise } = await openShellApproval(tc, 'tc-exact');

    // [t] opened the scope page; the first row is the exact gated segment.
    await tc.pressEnter();

    const response = (await responsePromise) as any;
    expect(response.outcome.outcome).toBe('selected');
    expect(response.outcome.optionId).toBe('always-accept');
    expect(response._meta?.kiro?.consent?.capability).toBe('shell');
    expect(response._meta?.kiro?.consent?.scope).toBe('session');
    // Exact trust persists the gated segment, never the whole compound command.
    expect(response._meta?.kiro?.consent?.resource).toBe(GATED_SHELL_SEGMENT);
    expect(response._meta?.kiro?.consent?.resource).not.toBe(
      COMPOUND_SHELL_COMMAND
    );
  }, 30000);

  it('[t] → entire-tool row sends the KAS wildcard resource', async () => {
    tc = liteKas('lite-consent-entire');
    const { responsePromise } = await openShellApproval(tc, 'tc-entire');

    // Rows: [exact, pattern, entire tool]. Two ↓ land on "Trust entire tool".
    await tc.sendKeys('\x1b[B');
    await tc.sleepMs(100);
    await tc.sendKeys('\x1b[B');
    await tc.sleepMs(100);
    await tc.pressEnter();

    const response = (await responsePromise) as any;
    expect(response.outcome.outcome).toBe('selected');
    expect(response.outcome.optionId).toBe('always-accept');
    expect(response._meta?.kiro?.consent).toEqual({
      capability: 'shell',
      scope: 'session',
      resource: '*',
    });
  }, 30000);

  it('[s] cycles persistence scope so the wire reply carries the chosen scope', async () => {
    tc = liteKas('lite-consent-scope-cycle');
    const { responsePromise } = await openShellApproval(tc, 'tc-scope');

    // session → workspace via one [s]; then confirm the exact row.
    await tc.sendKeys('s');
    await tc.sleepMs(150);
    await tc.waitForVisibleText('trust scope [workspace]', 3000);
    await tc.pressEnter();

    const response = (await responsePromise) as any;
    expect(response.outcome.optionId).toBe('always-accept');
    expect(response._meta?.kiro?.consent?.scope).toBe('workspace');
    expect(response._meta?.kiro?.consent?.resource).toBe(GATED_SHELL_SEGMENT);
  }, 30000);
});
