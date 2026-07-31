/**
 * ACP wire-level tests for the sandbox repo-attach notification.
 *
 * KAS (proto/connection-management-and-scoping and later) relays the
 * sandbox's bound-repository set to clients over the ACP wire as
 * `_meta.kiro.repositories` on a `session_info_update` — the BFF cannot
 * persist post-create attaches yet, so the live notification is the only
 * signal a mid-session `/repo` attach or agent-driven clone produces.
 *
 * These tests drive a MOCK KAS: they pin the CLI-side contract (parse the
 * meta, light the cloud footer, ignore it on local sessions) so that when
 * the sandbox fleet actually ships the notification, no CLI work remains.
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

const SESSION_ID = 'cloud-repos-session-1';

/**
 * Handshake for a CLOUD session: KAS advertises the cloud-sandbox execution
 * target + remote store, and `session/new` answers with a cloud placement, so
 * the CLI flips `cloudSessionActive` on — the gate the footer consumer sits
 * behind.
 */
function setupCloudHandshake(tc: AcpTestCase): void {
  tc.mock.on<InitializeRequest, InitializeResponse>('initialize', () => ({
    protocolVersion: 1,
    agentCapabilities: {
      sessionCapabilities: {},
      _meta: {
        kiro: {
          extensionMethods: [],
          executionTargets: ['local', 'cloud-sandbox'],
          sessionSources: ['local', 'remote'],
        },
      },
    },
  }));

  tc.mock.on<NewSessionRequest, NewSessionResponse>('session/new', () => ({
    sessionId: SESSION_ID,
    modes: defaultKasModes(),
    _meta: {
      kiro: {
        executionTarget: { kind: 'cloud-sandbox' },
        source: 'remote',
      },
    },
  }));

  // Cloud bring-up probes the source-provider catalog before creating the
  // session; a connected provider lets it proceed straight to session/new.
  tc.mock.on('_kiro/sourceProviders/list', () => ({
    providers: [
      {
        providerType: 'GITHUB',
        displayName: 'GitHub',
        connectionStatus: 'connected',
      },
    ],
  }));
  tc.mock.on('_kiro/sourceProviders/listResources', () => ({
    resources: [],
  }));

  tc.mock.on('session/set_config_option', () => ({}));
}

/** Handshake for a plain LOCAL session (no cloud capabilities). */
function setupLocalHandshake(tc: AcpTestCase): void {
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

/** Pushes the repo set the way KAS stamps it: `_meta.kiro.repositories`. */
function pushRepositories(
  tc: AcpTestCase,
  repositories: Array<Record<string, unknown>>
): void {
  tc.mock.notify('session/update', {
    sessionId: SESSION_ID,
    update: {
      sessionUpdate: 'session_info_update',
      _meta: { kiro: { repositories } },
    },
  });
}

describe('cloud repo-attach notification → footer', () => {
  let tc: AcpTestCase | null = null;

  afterEach(async () => {
    if (tc) await tc.cleanup();
    tc = null;
  });

  it('a pushed repositories set lights the cloud footer (repo, branch, +N others)', async () => {
    /**
     * GIVEN  a cloud session (KAS advertised cloud-sandbox; session/new
     *        answered with a cloud placement)
     * WHEN   KAS pushes session_info_update with _meta.kiro.repositories
     * THEN   the store's footer fields reflect the pushed set
     */
    tc = new AcpTestCase({
      testName: 'cloud-repo-notify',
      args: ['--cloud'],
      extraEnv: {
        // Cloud-sandbox surface is dark-shipped; the gate reads this flag.
        KIRO_ENABLED_FEATURES: JSON.stringify(['remote_sandbox']),
      },
    });
    setupCloudHandshake(tc);

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForStore((s) => s.cloudSessionActive === true, 5000);

    pushRepositories(tc, [
      {
        providerType: 'GITHUB',
        name: 'Xianwen-Peng/kiro-cli',
        branch: 'main',
        url: 'https://github.com/Xianwen-Peng/kiro-cli',
      },
      { providerType: 'GITHUB', name: 'Xianwen-Peng/second-repo' },
    ]);

    const store = await tc.waitForStore(
      (s) => s.cloudRepo === 'Xianwen-Peng/kiro-cli',
      5000
    );
    expect(store.cloudBranch).toBe('main');
    expect(store.cloudExtraRepos).toBe(1);
  }, 30000);

  it('a later empty repositories push clears the footer (detach-all)', async () => {
    tc = new AcpTestCase({
      testName: 'cloud-repo-notify-detach',
      args: ['--cloud'],
      extraEnv: {
        KIRO_ENABLED_FEATURES: JSON.stringify(['remote_sandbox']),
      },
    });
    setupCloudHandshake(tc);

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.waitForStore((s) => s.cloudSessionActive === true, 5000);

    pushRepositories(tc, [{ providerType: 'GITHUB', name: 'acme/app' }]);
    await tc.waitForStore((s) => s.cloudRepo === 'acme/app', 5000);

    pushRepositories(tc, []);
    const store = await tc.waitForStore((s) => s.cloudRepo === null, 5000);
    expect(store.cloudBranch).toBeNull();
    expect(store.cloudExtraRepos).toBe(0);
  }, 30000);

  it('a repositories push on a LOCAL session never touches the footer', async () => {
    tc = new AcpTestCase({ testName: 'local-repo-notify-ignored' });
    setupLocalHandshake(tc);

    await tc.launch();
    await tc.mock.awaitConnection();
    await tc.sleepMs(500);

    pushRepositories(tc, [{ providerType: 'GITHUB', name: 'acme/app' }]);
    await tc.sleepMs(500);

    const store = await tc.getStore();
    expect(store.cloudSessionActive).toBe(false);
    expect(store.cloudRepo).toBeNull();
  }, 30000);
});
