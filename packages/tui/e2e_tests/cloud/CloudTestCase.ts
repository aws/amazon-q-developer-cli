/**
 * Cloud-session E2E harness (batch 1).
 *
 * Wraps `E2ETestCase` to launch the TUI with the KAS agent engine against a
 * per-test mock Kiro Web Portal BFF (`mock-bff.mjs`), so cloud flows run
 * hermetically: real TUI + real KAS server (the published `@kiro/agent`
 * pinned in package.json — the same bundle nightly embeds) + fake BFF.
 * No network, no credentials, no real sandbox.
 *
 * KAS auth: `KIRO_API_KEY` in the environment. The KAS server prefers the
 * api-key env provider over `--auth=acp-callback` (see selectAuthProvider),
 * so the host-mediated `getAccessToken` callback — which needs a logged-in
 * CLI — is never exercised. The mock BFF ignores the bearer value.
 */
import { once } from 'node:events';
import * as net from 'node:net';
import * as path from 'node:path';
import { spawn, type ChildProcess } from 'node:child_process';
import { E2ETestCase, type E2ETestCaseBuilder } from '../E2ETestCase';

const TUI_DIR = path.join(__dirname, '../..');
const KAS_SERVER = path.join(
  TUI_DIR,
  'node_modules/@kiro/agent/dist/server/acp-server.js'
);
const MOCK_BFF = path.join(__dirname, 'mock-bff.mjs');

// Single source of truth, shared with mock-bff.mjs (see mock-space-ids.mjs) —
// no hand-kept sync contract. Re-exported so tests keep importing it from here.
export { MOCK_SPACE_IDS } from './mock-space-ids.mjs';

async function freePort(): Promise<number> {
  return await new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.listen(0, '127.0.0.1', () => {
      const port = (srv.address() as net.AddressInfo).port;
      srv.close(() => resolve(port));
    });
    srv.on('error', reject);
  });
}

export interface CloudHarnessOptions {
  /** Extra env for the mock BFF (e.g. MOCK_BFF_NO_PROVIDER=1). */
  bffEnv?: Record<string, string>;
  /** Extra CLI args after `chat` (e.g. ['--cloud', '--repo', 'x']). */
  cliArgs?: string[];
  /** Terminal size; defaults sized for the cloud connect checklist. */
  terminal?: { width: number; height: number };
  testName: string;
}

export class CloudHarness {
  private bff: ChildProcess | null = null;
  readonly bffPort: number;
  testCase: E2ETestCase | null = null;
  private bffStdout = '';

  private constructor(bffPort: number) {
    this.bffPort = bffPort;
  }

  /**
   * Everything the mock BFF printed so far (op log lines; full decoded
   * request bodies when the test launched with MOCK_BFF_LOG_BODIES=1).
   * Lets a test assert on wire content — e.g. that attached-file bytes
   * actually reached the BFF in-band.
   */
  bffOutput(): string {
    return this.bffStdout;
  }

  static async launch(opts: CloudHarnessOptions): Promise<CloudHarness> {
    const bffPort = await freePort();
    const h = new CloudHarness(bffPort);
    try {
      await h.startBff(opts.bffEnv ?? {});

      const builder: E2ETestCaseBuilder = E2ETestCase.builder()
        .withTerminal(opts.terminal ?? { width: 140, height: 42 })
        .withTestName(opts.testName)
        .withKasEngine()
        .withCliArgs(...(opts.cliArgs ?? ['--cloud']))
        .withEnv({
          // KAS engine, spawning the published server from node_modules.
          KIRO_AGENT_ENGINE: 'kas',
          KIRO_KAS_SERVER_PATH: KAS_SERVER,
          // The KAS session/list merge needs Node >= 20 (Array.toSorted) —
          // honor an explicit override so a machine whose default `node` is
          // older can point at a newer one; CI runners are already >= 20.
          KIRO_KAS_NODE_PATH: process.env.KIRO_KAS_NODE_PATH ?? 'node',
          // Point KAS at the per-test mock BFF; this is also what advertises
          // the cloud capabilities (executionTargets, sessionSources, providers).
          KIRO_REMOTE_SESSIONS_ENDPOINT: `http://127.0.0.1:${bffPort}`,
          // Deterministic auth with no logged-in CLI: KAS's env api-key
          // provider takes precedence over --auth=acp-callback, so the
          // host-token shell-out is never attempted. The mock BFF never
          // validates the bearer.
          KIRO_API_KEY: 'e2e-mock-api-key',
        });

      h.testCase = await builder.launch();
      return h;
    } catch (error) {
      await h.cleanup();
      throw error;
    }
  }

  private async startBff(env: Record<string, string>): Promise<void> {
    // Strip ambient MOCK_BFF_* toggles so a developer's exported vars can't
    // change a test's BFF behavior — each test states its toggles via bffEnv.
    const ambient = Object.fromEntries(
      Object.entries(process.env).filter(([k]) => !k.startsWith('MOCK_BFF_'))
    );
    this.bff = spawn('bun', [MOCK_BFF], {
      env: {
        ...ambient,
        MOCK_BFF_PORT: String(this.bffPort),
        ...env,
      },
      stdio: ['ignore', 'pipe', 'pipe'],
    });
    // Wait for the listen line so a slow spawn can't race the TUI boot.
    // Keep accumulating stdout afterwards so bffOutput() sees the full log.
    await new Promise<void>((resolve, reject) => {
      const t = setTimeout(
        () => reject(new Error('mock BFF did not boot in 10s')),
        10_000
      );
      this.bff!.stdout!.on('data', (d: Buffer) => {
        this.bffStdout += d.toString();
        if (this.bffStdout.includes('listening')) {
          clearTimeout(t);
          resolve();
        }
      });
      this.bff!.on('exit', (code) => {
        clearTimeout(t);
        reject(new Error(`mock BFF exited early (code ${code})`));
      });
    });
  }

  async cleanup(): Promise<void> {
    if (this.testCase) {
      await this.testCase.cleanup();
      this.testCase = null;
    }
    if (this.bff) {
      const bff = this.bff;
      this.bff = null;
      if (bff.exitCode === null && bff.signalCode === null) {
        const exited = once(bff, 'exit');
        bff.kill('SIGKILL');
        await exited;
      }
    }
  }
}
