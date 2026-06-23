/**
 * ACP integration tests for KAS granular permission consent flow.
 *
 * Validates that the real KasAcpClient, when receiving a
 * `session/request_permission` request from the mock server:
 *   1. Surfaces the approval dialog in the TUI
 *   2. Returns the correct `_meta.kiro.consent.scope` based on the user's choice
 *   3. Omits kiro.consent metadata when no consent context was sent (V2 mode)
 */
import { describe, it, expect, afterEach } from 'bun:test';
import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from './shared/AcpTestCase';
import { defaultKasModes } from './shared/default-agent';

const DOWN_ARROW = '\x1b[B';

function setupHandshake(
  tc: AcpTestCase,
  sessionId: string = 'test-session-1'
): void {
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

/** Sends a tool_call notification to create the tool message in the store. */
function sendToolCall(tc: AcpTestCase, toolCallId: string): void {
  tc.mock.notify('session/update', {
    sessionId: 'test-session-1',
    update: {
      sessionUpdate: 'tool_call',
      toolCallId,
      title: 'fs_write',
      kind: 'edit',
      rawInput: { path: '/tmp/test.ts', content: 'hello' },
    },
  });
}

/** Builds a standard permission request with KAS consent context. */
function makePermissionRequest(
  toolCallId: string,
  consent: Record<string, unknown>
) {
  return {
    sessionId: 'test-session-1',
    toolCall: { toolCallId },
    options: [
      { kind: 'allow_once', name: 'Allow Once', optionId: 'accept' },
      { kind: 'allow_always', name: 'Always Allow', optionId: 'always-accept' },
    ],
    _meta: { kiro: { consent } },
  };
}

/** Builds a V2-style permission request (no kiro.consent, has trustOptions). */
function makeV2PermissionRequest(toolCallId: string) {
  return {
    sessionId: 'test-session-1',
    toolCall: { toolCallId },
    options: [
      { kind: 'allow_once', name: 'Allow Once', optionId: 'accept' },
      { kind: 'allow_always', name: 'Always Allow', optionId: 'always-accept' },
    ],
    _meta: {
      trustOptions: [
        {
          label: 'Full command',
          display: 'echo hello',
          setting_key: 'allowedCommands',
          patterns: ['echo hello'],
        },
      ],
    },
  };
}

describe('KAS permission consent flow', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) {
      await tc.cleanup();
      tc = null;
    }
  });

  it('allow_once → response has scope "invocation"', async () => {
    tc = new AcpTestCase({ testName: 'consent-allow-once' });
    setupHandshake(tc);
    // No-op prompt handler (tests don't send user messages)
    tc.mock.on('session/prompt', () => ({ stopReason: 'end_turn' }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sleepMs(300);

    // Inject a tool call so the approval has a matching tool message
    sendToolCall(tc, 'tc-allow-once');
    await tc.sleepMs(200);

    // Server sends permission request with consent context
    const responsePromise = tc.mock.request(
      'session/request_permission',
      makePermissionRequest('tc-allow-once', {
        capability: 'fs_write',
        resource: 'src/index.ts',
      })
    );

    // Wait for approval dialog to appear
    await tc.waitForStore((s) => s.pendingApproval !== null, 5000);
    await tc.sleepMs(200);

    // Press Enter to select the first (highlighted) option: allow_once
    await tc.pressEnter();

    // Await the response from the TUI
    const response = (await responsePromise) as any;

    expect(response.outcome.outcome).toBe('selected');
    expect(response.outcome.optionId).toBe('accept');
    expect(response._meta?.kiro?.consent?.scope).toBe('invocation');
  }, 30000);

  it('allow_always → response has scope "session"', async () => {
    tc = new AcpTestCase({ testName: 'consent-allow-always' });
    setupHandshake(tc);
    tc.mock.on('session/prompt', () => ({ stopReason: 'end_turn' }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sleepMs(300);

    sendToolCall(tc, 'tc-allow-always');
    await tc.sleepMs(200);

    const responsePromise = tc.mock.request(
      'session/request_permission',
      makePermissionRequest('tc-allow-always', {
        capability: 'fs_write',
        resource: '/workspace/src',
        workspaceRoot: '/workspace',
      })
    );

    await tc.waitForStore((s) => s.pendingApproval !== null, 5000);
    await tc.sleepMs(200);

    // Navigate down to "Trust, always allow in this session" (second option)
    await tc.sendKeys(DOWN_ARROW);
    await tc.sleepMs(100);
    // Enter opens the kas-scope sub-page (because consent context is present)
    await tc.pressEnter();
    await tc.sleepMs(200);
    // First item on kas-scope page is "This session" → scope 'session'
    await tc.pressEnter();

    const response = (await responsePromise) as any;

    expect(response.outcome.outcome).toBe('selected');
    expect(response.outcome.optionId).toBe('always-accept');
    expect(response._meta?.kiro?.consent?.scope).toBe('session');
    expect(response._meta?.kiro?.consent?.resource).toBe('/workspace/src');
    expect(response._meta?.kiro?.consent?.workspaceRoot).toBe('/workspace');
  }, 30000);

  it('consent context populates the approval store correctly', async () => {
    tc = new AcpTestCase({ testName: 'consent-context-populated' });
    setupHandshake(tc);
    tc.mock.on('session/prompt', () => ({ stopReason: 'end_turn' }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sleepMs(300);

    sendToolCall(tc, 'tc-context');
    await tc.sleepMs(200);

    // Fire the request; we just need the approval in the store
    const responsePromise = tc.mock.request(
      'session/request_permission',
      makePermissionRequest('tc-context', {
        capability: 'fs_write',
        resource: 'src/index.ts',
        askType: 'implicit',
      })
    );

    const store = await tc.waitForStore(
      (s) => s.pendingApproval !== null,
      5000
    );

    expect(store.pendingApproval!.consentContext).toBeDefined();
    expect(store.pendingApproval!.consentContext!.capability).toBe('fs_write');
    expect(store.pendingApproval!.consentContext!.resource).toBe(
      'src/index.ts'
    );

    // Dismiss the approval so cleanup doesn't leave a dangling request
    await tc.pressEscape();
    // Swallow the expected rejection (cancelled approval)
    await responsePromise.catch(() => {});
  }, 30000);

  it('no consent context → response still includes scope (KAS default)', async () => {
    tc = new AcpTestCase({ testName: 'consent-v2-no-consent' });
    setupHandshake(tc);
    tc.mock.on('session/prompt', () => ({ stopReason: 'end_turn' }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sleepMs(300);

    sendToolCall(tc, 'tc-v2');
    await tc.sleepMs(200);

    const responsePromise = tc.mock.request(
      'session/request_permission',
      makeV2PermissionRequest('tc-v2')
    );

    await tc.waitForStore((s) => s.pendingApproval !== null, 5000);
    await tc.sleepMs(200);

    // Select allow_once (first option)
    await tc.pressEnter();

    const response = (await responsePromise) as any;

    expect(response.outcome.outcome).toBe('selected');
    expect(response.outcome.optionId).toBe('accept');
    // KAS mode always attaches consent scope even without consent context
    expect(response._meta?.kiro?.consent?.scope).toBe('invocation');
  }, 30000);

  it('shows tool name from permission request title when tool_call notification missing', async () => {
    tc = new AcpTestCase({ testName: 'consent-tool-title-fallback' });
    setupHandshake(tc);
    tc.mock.on('session/prompt', () => ({ stopReason: 'end_turn' }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sleepMs(300);

    // Do NOT send tool_call notification — simulate race where permission arrives first
    // Send permission request with a title
    const responsePromise = tc.mock.request('session/request_permission', {
      sessionId: 'test-session-1',
      toolCall: {
        toolCallId: 'tc-title-test',
        title: 'Run Command',
        status: 'pending',
      },
      options: [{ kind: 'allow_once', name: 'Allow', optionId: 'accept' }],
      _meta: {},
    });

    await tc.waitForStore((s) => s.pendingApproval !== null, 5000);
    await tc.sleepMs(200);

    // The approval dialog should show "Run Command requires approval" not "Tool requires approval"
    await tc.waitForVisibleText('Run Command', 3000);

    // Dismiss to clean up
    await tc.pressEnter();
    await responsePromise;
  }, 30000);

  it('renders a user_input question (markdown body, no "requires approval")', async () => {
    tc = new AcpTestCase({ testName: 'consent-user-input-question' });
    setupHandshake(tc);
    tc.mock.on('session/prompt', () => ({ stopReason: 'end_turn' }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sleepMs(300);

    // A spec design question: reuses the requestPermission channel but carries
    // no tool (no `_meta.kiro.toolId`), so it renders as a question rather than
    // a tool approval. Title is markdown.
    const responsePromise = tc.mock.request('session/request_permission', {
      sessionId: 'test-session-1',
      toolCall: {
        toolCallId: 'tc-question',
        title: '**Requirement 7.3:** What should happen on retry?',
        status: 'pending',
      },
      options: [
        {
          kind: 'allow_once',
          name: 'Always return an error',
          optionId: 'opt-a',
        },
        {
          kind: 'allow_once',
          name: 'Only after retries exhausted',
          optionId: 'opt-b',
        },
      ],
    });

    await tc.waitForStore((s) => s.pendingApproval !== null, 5000);
    await tc.sleepMs(200);

    // Question text is shown; the markdown header text renders (bold marker
    // consumed, not shown raw). The "requires approval" wording is absent.
    await tc.waitForVisibleText('Requirement 7.3', 3000);
    const snapshot = tc.getSnapshotFormatted();
    expect(snapshot).not.toContain('requires approval');
    expect(snapshot).toContain('Question');

    await tc.pressEnter();
    await responsePromise;
  }, 30000);

  it('compound shell: exact-trust persists the GATED segment, not the whole command', async () => {
    // Regression guard (real wire) for the v3+KAS compound-shell trust loop.
    // A compound command like `git status && echo "done"` is gated one segment
    // at a time: the whole command arrives as consent.resource, but the segment
    // needing consent right now is consent.triggeringResource. The exact-trust
    // option the UI sends back MUST be that gated segment — otherwise the trust
    // never matches the re-asked segment and the policy re-prompts forever.
    //
    // This drives the real approval menu (allow_always → kas-scope page → the
    // first/exact item) and asserts the reply that crosses the wire carries the
    // gated segment in _meta.kiro.consent.resource. Pre-fix it carried the whole
    // command (kasResource: resource instead of exactResource).
    const COMPOUND = 'git status && echo "done"';
    const GATED = 'echo "done"';

    tc = new AcpTestCase({ testName: 'consent-compound-shell-exact' });
    setupHandshake(tc);
    tc.mock.on('session/prompt', () => ({ stopReason: 'end_turn' }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);
    await tc.sleepMs(300);

    // Inject a shell tool_call so the approval has a matching tool message.
    tc.mock.notify('session/update', {
      sessionId: 'test-session-1',
      update: {
        sessionUpdate: 'tool_call',
        toolCallId: 'tc-compound-shell',
        title: 'shell',
        kind: 'execute',
        rawInput: { command: COMPOUND },
      },
    });
    await tc.sleepMs(200);

    // Permission request carries the compound command as `resource` and the
    // gated sub-command as `triggeringResource`. `toolId` marks it a real tool
    // approval (not a user_input question), matching the KAS shell-approval
    // wire shape.
    const responsePromise = tc.mock.request('session/request_permission', {
      sessionId: 'test-session-1',
      toolCall: { toolCallId: 'tc-compound-shell' },
      options: [
        { kind: 'allow_once', name: 'Allow Once', optionId: 'accept' },
        {
          kind: 'allow_always',
          name: 'Always Allow',
          optionId: 'always-accept',
        },
      ],
      _meta: {
        kiro: {
          toolId: 'shell-tool',
          consent: {
            capability: 'shell',
            resource: COMPOUND,
            triggeringResource: GATED,
          },
        },
      },
    });

    await tc.waitForStore((s) => s.pendingApproval !== null, 5000);
    await tc.sleepMs(200);

    // Navigate down to "Trust, always allow in this session" (allow_always).
    await tc.sendKeys(DOWN_ARROW);
    await tc.sleepMs(100);
    // Enter opens the kas-scope sub-page (consent context present, KAS engine).
    await tc.pressEnter();
    // Wait for the kas-scope page (its title reads "trust [session]") rather
    // than a bare sleep, to ride out the `Menu key={page}` remount.
    await tc.waitForVisibleText('trust [session]', 3000);
    await tc.sleepMs(200);
    // First item on the kas-scope page is the exact-match trust for the gated
    // segment (resourceLabel is truthy). Enter selects it.
    await tc.pressEnter();

    const response = (await responsePromise) as any;

    expect(response.outcome.outcome).toBe('selected');
    expect(response.outcome.optionId).toBe('always-accept');
    // The trusted resource must be the GATED segment...
    expect(response._meta?.kiro?.consent?.resource).toBe(GATED);
    // ...never the whole compound command (the pre-fix regression value).
    expect(response._meta?.kiro?.consent?.resource).not.toBe(COMPOUND);
  }, 30000);
});
