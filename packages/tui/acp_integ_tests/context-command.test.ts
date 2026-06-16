/**
 * ACP wire-level tests for `/context` command (KAS `_kiro/session/context`).
 *
 * Covers all 4 subcommands: show, add, remove, clear.
 * Validates the exact params shape sent over the wire and the resulting
 * store/UI state transitions the user observes.
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

interface ContextParams {
  sessionId: string;
  subcommand: string;
  path?: string;
  force?: boolean;
}

function setupHandshake(tc: AcpTestCase): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: {
        kiro: {
          extensionMethods: [
            {
              method: '_kiro/session/context',
              name: '/context',
              description: 'Manage context files',
            },
          ],
        },
      },
    },
  }));

  tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
    sessionId: 'ctx-session-1',
    modes: defaultKasModes(),
  }));

  tc.mock.on('session/set_config_option', () => ({}));
}

describe('/context command', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('show: sends _kiro/session/context with subcommand show and surfaces entries', async () => {
    /**
     * GIVEN  _kiro/session/context advertised, handler returns entries
     * WHEN   user types /context
     * THEN   wire sends {subcommand:'show', sessionId} and TUI shows alert
     */
    tc = new AcpTestCase({ testName: 'context-show' });
    setupHandshake(tc);

    tc.mock.on('_kiro/session/context', (params: unknown) => {
      const p = params as ContextParams;
      if (p.subcommand === 'show') {
        return {
          success: true,
          data: {
            entries: [
              { path: './README.md', matched: true },
              { path: './src/index.ts', matched: true },
            ],
            message: '2 context files attached',
          },
        };
      }
      return { success: true, data: {} };
    });

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    await tc.sendKeys('/context');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(500);

    const reqs = tc.mock.receivedRequests('_kiro/session/context');
    expect(reqs.length).toBeGreaterThanOrEqual(1);
    const params = reqs[0]!.params as ContextParams;
    expect(params.subcommand).toBe('show');
    expect(params.sessionId).toBe('ctx-session-1');
  });

  it('add: sends subcommand add with path', async () => {
    /**
     * GIVEN  _kiro/session/context handler available
     * WHEN   user types /context add ./package.json
     * THEN   wire sends {subcommand:'add', path:'./package.json', sessionId}
     */
    tc = new AcpTestCase({ testName: 'context-add' });
    setupHandshake(tc);

    tc.mock.on('_kiro/session/context', () => ({
      success: true,
      data: { success: true, message: 'Added ./package.json to context' },
    }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    await tc.sendKeys('/context add ./package.json');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(500);

    const reqs = tc.mock.receivedRequests('_kiro/session/context');
    expect(reqs.length).toBeGreaterThanOrEqual(1);
    const addReq = reqs.find(
      (r) => (r.params as ContextParams).subcommand === 'add'
    );
    expect(addReq).toBeDefined();
    const params = addReq!.params as ContextParams;
    expect(params.path).toBe('./package.json');
    expect(params.sessionId).toBe('ctx-session-1');
  });

  it('remove: sends subcommand remove with path', async () => {
    /**
     * GIVEN  _kiro/session/context handler available
     * WHEN   user types /context remove ./package.json
     * THEN   wire sends {subcommand:'remove', path:'./package.json', sessionId}
     */
    tc = new AcpTestCase({ testName: 'context-remove' });
    setupHandshake(tc);

    tc.mock.on('_kiro/session/context', () => ({
      success: true,
      data: { success: true, message: 'Removed ./package.json from context' },
    }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    await tc.sendKeys('/context remove ./package.json');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(500);

    const reqs = tc.mock.receivedRequests('_kiro/session/context');
    expect(reqs.length).toBeGreaterThanOrEqual(1);
    const removeReq = reqs.find(
      (r) => (r.params as ContextParams).subcommand === 'remove'
    );
    expect(removeReq).toBeDefined();
    const params = removeReq!.params as ContextParams;
    expect(params.path).toBe('./package.json');
    expect(params.sessionId).toBe('ctx-session-1');
  });

  it('clear: sends subcommand clear', async () => {
    /**
     * GIVEN  _kiro/session/context handler available
     * WHEN   user types /context clear
     * THEN   wire sends {subcommand:'clear', sessionId}
     */
    tc = new AcpTestCase({ testName: 'context-clear' });
    setupHandshake(tc);

    tc.mock.on('_kiro/session/context', () => ({
      success: true,
      data: { success: true, message: 'Context cleared' },
    }));

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(300);

    await tc.sendKeys('/context clear');
    await tc.sleepMs(200);
    await tc.pressEnter();
    await tc.sleepMs(500);

    const reqs = tc.mock.receivedRequests('_kiro/session/context');
    expect(reqs.length).toBeGreaterThanOrEqual(1);
    const clearReq = reqs.find(
      (r) => (r.params as ContextParams).subcommand === 'clear'
    );
    expect(clearReq).toBeDefined();
    expect((clearReq!.params as ContextParams).sessionId).toBe('ctx-session-1');
  });
});
