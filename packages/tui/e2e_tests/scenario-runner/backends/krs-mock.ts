import { spawn, type ChildProcess } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import { existsSync, mkdtempSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import { E2ETestCase } from '../../E2ETestCase';
import { type KrsPlan, resolveKrsPlan } from '../krs-plan';
import type {
  Engine,
  RunOptions,
  Scenario,
  ScenarioBackend,
  TestHarness,
} from '../types';
import { LiveHarness } from './live';

/**
 * Real KAS, fake model.
 *
 * The `live` backend runs everything for real; `acp-mock` replaces the agent
 * entirely with a recorded ACP fixture. This sits between them: a real KAS, real
 * agent loop, real tools, real ACP, with only the model's words coming from a
 * script. That is what makes a KAS version bump verifiable — the artifact under
 * test is the published one, and the only thing held still is the model.
 *
 * The turns come from `resolveKrsPlan`, which reads them from
 * `fixtures/krs/<scenario-id>.json`. A scenario states that it belongs to this
 * backend by having such a file; this backend reads no scenario fields itself,
 * and nothing is inferred from the scenario's steps. A scenario with no turns
 * fails at launch rather than part-way through a run.
 */

const REPO_ROOT = join(import.meta.dir, '../../../../..');
const READY_TIMEOUT_MS = 20_000;

function resolveServerBinary(): string {
  const override = process.env.MOCK_KRS_BIN;
  if (override) {
    if (!existsSync(override)) {
      throw new Error(`MOCK_KRS_BIN points at a missing file: ${override}`);
    }
    return override;
  }

  const candidates = [
    join(REPO_ROOT, 'target/release/mock-krs-server'),
    join(REPO_ROOT, 'target/debug/mock-krs-server'),
  ];
  const found = candidates.find((candidate) => existsSync(candidate));
  if (!found) {
    throw new Error(
      'mock-krs-server is not built. Run `cargo build -p mock-krs-server` ' +
        '(or set MOCK_KRS_BIN to a binary).'
    );
  }
  return found;
}

/** A fake KRS listening on an ephemeral port. */
export class MockKrsProcess {
  private constructor(
    private readonly proc: ChildProcess,
    readonly endpoint: string,
    readonly apiKey: string
  ) {}

  static async start(): Promise<MockKrsProcess> {
    // A per-run key: the server requires a bearer token, and using the same value
    // on both sides means a mismatch is a wiring bug rather than a mystery 403.
    const apiKey = `krs-mock-${randomBytes(8).toString('hex')}`;
    const portFile = join(mkdtempSync(join(tmpdir(), 'krs-mock-')), 'port');

    const proc = spawn(
      resolveServerBinary(),
      ['--port', '0', '--api-key', apiKey, '--port-file', portFile],
      { stdio: ['ignore', 'pipe', 'pipe'] }
    );

    let stderr = '';
    proc.stderr?.on('data', (chunk) => {
      stderr += chunk.toString();
    });

    const deadline = Date.now() + READY_TIMEOUT_MS;
    while (Date.now() < deadline) {
      if (proc.exitCode !== null) {
        throw new Error(
          `mock-krs-server exited with ${proc.exitCode} before binding.\n${stderr}`
        );
      }
      try {
        const port = readFileSync(portFile, 'utf8').trim();
        if (port) {
          return new MockKrsProcess(proc, `http://127.0.0.1:${port}`, apiKey);
        }
      } catch {
        // Not written yet.
      }
      await new Promise((resolve) => setTimeout(resolve, 50));
    }

    proc.kill();
    throw new Error(
      `mock-krs-server did not report a port within ${READY_TIMEOUT_MS}ms.\n${stderr}`
    );
  }

  /** Enqueues a scenario's turns, exactly as written. */
  async loadPlan(plan: KrsPlan): Promise<KrsLoadReport> {
    const response = await fetch(`${this.endpoint}/__control/turns`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ turns: plan.turns }),
    });
    if (!response.ok) {
      throw new Error(
        `fake KRS rejected the turns in ${plan.sidecarPath}: ${response.status} ${await response.text()}`
      );
    }
    return (await response.json()) as KrsLoadReport;
  }

  /** Captured requests, for assertions about what KAS actually sent. */
  async requests(): Promise<unknown[]> {
    const response = await fetch(`${this.endpoint}/__control/requests`);
    const body = (await response.json()) as { requests: unknown[] };
    return body.requests;
  }

  stop(): void {
    this.proc.kill();
  }
}

export interface KrsLoadReport {
  queued: number;
}

/** Delegates to the live harness, and stops the fake KRS on cleanup. */
class KrsMockHarness implements TestHarness {
  constructor(
    private readonly inner: LiveHarness,
    private readonly server: MockKrsProcess
  ) {}

  sendKeys(input: string | number[]): Promise<void> {
    return this.inner.sendKeys(input);
  }

  pressEnter(): Promise<void> {
    return this.inner.pressEnter();
  }

  pressEscape(): Promise<void> {
    return this.inner.pressEscape();
  }

  pressCtrlC(): Promise<void> {
    return this.inner.pressCtrlC();
  }

  pressCtrlCTwice(): Promise<void> {
    return this.inner.pressCtrlCTwice();
  }

  sleepMs(ms: number): Promise<void> {
    return this.inner.sleepMs(ms);
  }

  getStore() {
    return this.inner.getStore();
  }

  getSnapshot(): string[] {
    return this.inner.getSnapshot();
  }

  getSnapshotHtml(): string {
    return this.inner.getSnapshotHtml();
  }

  waitForText(text: string, timeout?: number): Promise<void> {
    return this.inner.waitForText(text, timeout);
  }

  waitForIdle(timeout?: number): Promise<void> {
    return this.inner.waitForIdle(timeout);
  }

  expectExit(timeout?: number): Promise<number> {
    return this.inner.expectExit(timeout);
  }

  async cleanup(): Promise<void> {
    try {
      await this.inner.cleanup();
    } finally {
      // Always release the port, even if the TUI teardown throws.
      this.server.stop();
    }
  }
}

export function createKrsMockBackend(engine: Engine = 'kas'): ScenarioBackend {
  if (engine !== 'kas') {
    // Only the KAS engine talks to KRS; v2 has its own client and its own
    // endpoint override.
    throw new Error(`backend "krs-mock" only supports engine "kas", got "${engine}"`);
  }

  return {
    id: 'krs-mock',
    engine,
    async launch(scenario: Scenario, opts: RunOptions): Promise<TestHarness> {
      // Resolved up front, so this backend never reads scenario fields itself.
      const plan = resolveKrsPlan(scenario);

      const server = await MockKrsProcess.start();
      try {
        await server.loadPlan(plan);

        const testCase = await E2ETestCase.builder()
          .withTestName(`scenario-${scenario.id}-krs-mock-${Date.now()}`)
          .withTerminal(opts.terminal ?? { width: 120, height: 40 })
          .withTimeout(opts.timeout ?? scenario.timeout ?? 120_000)
          .withKasEngine()
          .withEnv({
            KIRO_AGENT_ENGINE: 'kas',
            // Appended by kas.ts as KAS's `--endpoint`.
            KIRO_KAS_ENDPOINT: server.endpoint,
            // Read by KAS ahead of every auth mode, so no login is needed.
            KIRO_API_KEY: server.apiKey,
          })
          .launch();

        return new KrsMockHarness(new LiveHarness(testCase), server);
      } catch (error) {
        server.stop();
        throw error;
      }
    },
  };
}
