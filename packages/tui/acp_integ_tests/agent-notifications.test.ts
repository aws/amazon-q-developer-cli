/**
 * ACP wire-level tests for all KAS agent → client notifications.
 *
 * Covers:
 * - _kiro/customAgent/not_found (mode fallback)
 * - _kiro/customAgent/config_error (agent config parse error)
 * - _kiro/error/rate_limit (rate limit error)
 * - _kiro/mcp/governance_disabled (MCP governance unavailable)
 * - _kiro/governance/state (unified governance — web tools toggle)
 * - current_mode_update via session/update (mode change push)
 */
import { describe, it, expect, afterEach } from 'bun:test';
import type {
  InitializeRequest,
  InitializeResponse,
  NewSessionRequest,
  NewSessionResponse,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from './shared/AcpTestCase';

function setupHandshake(tc: AcpTestCase): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: { kiro: { extensionMethods: [] } },
    },
  }));

  tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
    sessionId: 'notif-session-1',
    modes: {
      currentModeId: 'vibe',
      availableModes: [
        { id: 'vibe', name: 'Default' },
        { id: 'spec', name: 'Spec' },
      ],
    },
  }));

  tc.mock.on('session/set_config_option', () => ({}));
}

describe('KAS agent notifications', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('_kiro/customAgent/not_found updates initErrors AND currentAgent to fallback', async () => {
    /**
     * GIVEN  TUI connected with mode 'vibe'
     * WHEN   server pushes _kiro/customAgent/not_found with fallbackAgent
     * THEN   store.initErrors contains the not_found entry
     *        store.currentAgent updates to the fallback agent
     *
     * This test validates the fix for BUG-3: KasAcpClient now broadcasts
     * AgentSwitched after updating its internal mode cache, so the store's
     * currentAgent reflects the fallback immediately.
     */
    tc = new AcpTestCase({ testName: 'agent-not-found' });
    // Use a custom initial mode so fallback to 'vibe' triggers a real switch
    tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
      protocolVersion: 1,
      agentCapabilities: {
        sessionCapabilities: {},
        _meta: { kiro: { extensionMethods: [] } },
      },
    }));
    tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
      sessionId: 'notif-session-1',
      modes: {
        currentModeId: 'my-custom-agent',
        availableModes: [
          { id: 'vibe', name: 'Default' },
          { id: 'my-custom-agent', name: 'Custom' },
        ],
      },
    }));
    tc.mock.on('session/set_config_option', () => ({}));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    tc.mock.notify('_kiro/customAgent/not_found', {
      requestedAgent: 'nonexistent-agent',
      fallbackAgent: 'vibe',
      message: 'Agent not found, falling back to default',
    });
    await tc.sleepMs(300);

    const store = await tc.getStore();
    // initErrors should contain the not_found entry
    expect(store.initErrors.length).toBeGreaterThanOrEqual(1);
    const notFoundErr = store.initErrors.find(
      (e) => e.type === 'agent_not_found'
    );
    expect(notFoundErr).toBeDefined();
    expect(notFoundErr!.requestedAgent).toBe('nonexistent-agent');
    expect(notFoundErr!.fallbackAgent).toBe('vibe');
    // currentAgent should now reflect the fallback (BUG-3 fix)
    // Note: fromKasModeId maps 'vibe' → 'kiro_default' for display
    expect(store.currentAgent?.name).toBe('kiro_default');
  });

  it('_kiro/customAgent/config_error is suppressed (no-op)', async () => {
    /**
     * GIVEN  TUI connected
     * WHEN   server pushes _kiro/customAgent/config_error
     * THEN   no error is surfaced (suppressed due to false positives)
     */
    tc = new AcpTestCase({ testName: 'agent-config-error' });
    setupHandshake(tc);

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    tc.mock.notify('_kiro/customAgent/config_error', {
      agentName: 'my-broken-agent',
      error: 'Invalid YAML: unexpected end of stream',
    });
    await tc.sleepMs(300);

    const store = await tc.getStore();
    expect(store.initErrors).toHaveLength(0);
    expect(store.transientAlert).toBeNull();
  });

  it('_kiro/error/rate_limit surfaces rate limit message via transientAlert', async () => {
    /**
     * GIVEN  TUI connected
     * WHEN   server pushes _kiro/error/rate_limit
     * THEN   store.transientAlert contains the rate limit message
     *
     * This test validates the fix for BUG-2: kiro.ts now forwards
     * RateLimitError events through initNotificationHandler at idle time.
     */
    tc = new AcpTestCase({ testName: 'rate-limit' });
    setupHandshake(tc);

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    tc.mock.notify('_kiro/error/rate_limit', {
      message: 'Rate limit exceeded. Please wait 30 seconds.',
      retryAfterMs: 30000,
    });

    const store = await tc.waitForStore((s) => s.transientAlert !== null, 3000);
    expect(store.transientAlert!.message).toContain('Rate limit');
  });

  it('_kiro/mcp/governance_disabled is handled without crash', async () => {
    /**
     * GIVEN  TUI connected
     * WHEN   server pushes _kiro/mcp/governance_disabled
     * THEN   TUI handles it gracefully (no crash, store accessible)
     */
    tc = new AcpTestCase({ testName: 'mcp-governance' });
    setupHandshake(tc);

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    tc.mock.notify('_kiro/mcp/governance_disabled', {
      reason: 'api_failure',
      message: 'Governance API unavailable',
    });
    await tc.sleepMs(300);

    // Should not crash — store should still be accessible
    const store = await tc.getStore();
    expect(store).toBeDefined();
  });

  it('_kiro/governance/state with webToolsEnabled=false surfaces web tools disabled', async () => {
    /**
     * GIVEN  TUI connected (KAS path)
     * WHEN   server pushes _kiro/governance/state with features.webToolsEnabled=false
     * THEN   store.initErrors gains a web_tools_governance_disabled entry (apiFailure=false
     *        for an explicit admin toggle)
     */
    tc = new AcpTestCase({ testName: 'governance-state-web-tools-off' });
    setupHandshake(tc);

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    tc.mock.notify('_kiro/governance/state', {
      sessionId: 'notif-session-1',
      isEnterprise: true,
      features: {
        mcpEnabled: true,
        webToolsEnabled: false,
        autonomousAgents: true,
        usageAnalytics: false,
        promptLogging: false,
        codeReferenceTracker: false,
        contentCollection: false,
      },
      disabledReason: 'admin_disabled',
    });
    await tc.sleepMs(300);

    const store = await tc.getStore();
    const webToolsErr = store.initErrors.find(
      (e: { type: string }) => e.type === 'web_tools_governance_disabled'
    ) as { type: string; apiFailure: boolean } | undefined;
    expect(webToolsErr).toBeDefined();
    expect(webToolsErr?.apiFailure).toBe(false);
  });

  it('_kiro/governance/state with webToolsEnabled=true does not surface a warning', async () => {
    /**
     * GIVEN  TUI connected (KAS path)
     * WHEN   server pushes _kiro/governance/state with features.webToolsEnabled=true
     * THEN   no web_tools_governance_disabled entry is added
     */
    tc = new AcpTestCase({ testName: 'governance-state-web-tools-on' });
    setupHandshake(tc);

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    tc.mock.notify('_kiro/governance/state', {
      sessionId: 'notif-session-1',
      isEnterprise: true,
      features: {
        mcpEnabled: true,
        webToolsEnabled: true,
        autonomousAgents: true,
        usageAnalytics: false,
        promptLogging: false,
        codeReferenceTracker: false,
        contentCollection: false,
      },
    });
    await tc.sleepMs(300);

    const store = await tc.getStore();
    const webToolsErr = store.initErrors.find(
      (e: { type: string }) => e.type === 'web_tools_governance_disabled'
    );
    expect(webToolsErr).toBeUndefined();
  });

  it('current_mode_update via session/update changes store mode', async () => {
    /**
     * GIVEN  TUI connected with mode 'vibe'
     * WHEN   server pushes current_mode_update with 'spec'
     * THEN   store.currentAgent reflects the new mode
     */
    tc = new AcpTestCase({ testName: 'mode-update' });
    setupHandshake(tc);

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    tc.mock.notify('session/update', {
      sessionId: 'notif-session-1',
      update: {
        sessionUpdate: 'current_mode_update',
        currentModeId: 'spec',
      },
    });

    const store = await tc.waitForStore(
      (s) => s.currentAgent?.name === 'spec',
      3000
    );
    expect(store.currentAgent?.name).toBe('spec');
  });
});
