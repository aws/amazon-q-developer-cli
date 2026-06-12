/**
 * ACP wire-level tests for /spec command (_kiro/spec/resolveSession + invoke).
 * Uses temp workspace with .kiro/specs/ fixtures.
 */
import { describe, it, expect, afterEach, beforeEach } from 'bun:test';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
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
    sessionId: 'spec-session-1',
    modes: {
      currentModeId: 'vibe',
      availableModes: [{ id: 'vibe', name: 'Default' }],
    },
  }));
  tc.mock.on('session/set_config_option', () => ({}));
}

describe('/spec command (_kiro/spec/*)', () => {
  let tc: AcpTestCase | null = null;
  let workspaceDir: string;

  beforeEach(() => {
    workspaceDir = mkdtempSync(join(tmpdir(), 'kiro-spec-test-'));
    const specDir = join(workspaceDir, '.kiro', 'specs', 'my-feature');
    mkdirSync(specDir, { recursive: true });
    writeFileSync(
      join(specDir, 'requirements.md'),
      '# Requirements\n\n- Feature does X'
    );
    writeFileSync(join(specDir, 'tasks.md'), '# Tasks\n\n- [ ] Implement X');
  });

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
    try {
      rmSync(workspaceDir, { recursive: true, force: true });
    } catch {
      /* ignore */
    }
  });

  it('/spec run calls resolveSession with featureName', async () => {
    /**
     * GIVEN  workspace has .kiro/specs/my-feature/
     * WHEN   /spec run my-feature
     * THEN   _kiro/spec/resolveSession called with featureName
     */
    tc = new AcpTestCase({ testName: 'spec-resolve', cwd: workspaceDir });
    setupHandshake(tc);
    tc.mock.on('_kiro/spec/resolveSession', () => ({
      sessionId: 'spec-feat-1',
    }));
    tc.mock.on('_kiro/spec/invoke', () => ({ success: true }));
    tc.mock.on('session/prompt', () => ({ stopReason: 'end_turn' }) as any);

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);

    await tc.sendKeys('/spec run my-feature');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(1500);

    const reqs = tc.mock.receivedRequests('_kiro/spec/resolveSession');
    expect(reqs.length).toBeGreaterThanOrEqual(1);
    expect((reqs[0]!.params as any).featureName).toBe('my-feature');
  });

  it('/spec run calls invoke with runAllTasks operation', async () => {
    /**
     * GIVEN  resolveSession succeeds
     * WHEN   /spec run dispatches
     * THEN   _kiro/spec/invoke called with operation + sessionId
     */
    tc = new AcpTestCase({ testName: 'spec-invoke', cwd: workspaceDir });
    setupHandshake(tc);
    tc.mock.on('_kiro/spec/resolveSession', () => ({
      sessionId: 'spec-feat-2',
    }));
    tc.mock.on('_kiro/spec/invoke', () => ({ success: true }));
    tc.mock.on('session/prompt', () => ({ stopReason: 'end_turn' }) as any);

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);

    await tc.sendKeys('/spec run my-feature');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(1500);

    const reqs = tc.mock.receivedRequests('_kiro/spec/invoke');
    expect(reqs.length).toBeGreaterThanOrEqual(1);
    const p = reqs[0]!.params as any;
    expect(p.sessionId).toBe('spec-feat-2');
    expect(p.operation).toBe('runAllTasks');
    expect(p.featureName).toBe('my-feature');
  });

  it('/spec new switches to spec mode and sends prompt', async () => {
    /**
     * GIVEN  TUI ready
     * WHEN   /spec new my-feature
     * THEN   set_config_option(mode:'spec') sent + prompt contains feature name
     */
    tc = new AcpTestCase({ testName: 'spec-new', cwd: workspaceDir });
    setupHandshake(tc);
    tc.mock.on('session/prompt', async () => {
      if (!tc) return { stopReason: 'end_turn' } as any;
      tc.mock.notify('session/update', {
        sessionId: 'spec-session-1',
        update: {
          sessionUpdate: 'agent_message_chunk',
          content: { type: 'text', text: 'Creating spec...' },
        },
      });
      await new Promise((r) => setTimeout(r, 100));
      if (!tc) return { stopReason: 'end_turn' } as any;
      tc.mock.notify('session/update', {
        sessionId: 'spec-session-1',
        update: {
          sessionUpdate: 'session_info_update',
          _meta: { kiro: { kind: 'turn_completion' } },
        },
      });
      return { stopReason: 'end_turn' } as unknown as any;
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForVisibleText('ask a question', 10000);

    await tc.sendKeys('/spec new my-feature');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(1500);

    const configReqs = tc.mock.receivedRequests('session/set_config_option');
    const modeReqs = configReqs.filter(
      (r) => (r.params as any).configId === 'mode'
    );
    expect(modeReqs.length).toBeGreaterThanOrEqual(1);
    expect((modeReqs[0]!.params as any).value).toBe('spec');

    const promptReqs = tc.mock.receivedRequests('session/prompt');
    expect(promptReqs.length).toBeGreaterThanOrEqual(1);
    const content = JSON.stringify(promptReqs[0]!.params);
    expect(content).toContain('my-feature');
  });
});
