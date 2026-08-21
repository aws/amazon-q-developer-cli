import { spawn, type ChildProcess } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';

import { E2ETestCase } from '../../E2ETestCase';
import type {
  Engine,
  RunOptions,
  Scenario,
  ScenarioBackend,
  TestHarness,
  VerifyResult,
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
 * The turns come from the scenario itself, and nothing is inferred from its
 * steps: a scenario that cannot state what the model says cannot run here.
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
  async loadTurns(scenario: Scenario): Promise<KrsLoadReport> {
    const response = await fetch(`${this.endpoint}/__control/turns`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ turns: scenario.turns }),
    });
    if (!response.ok) {
      throw new Error(
        `fake KRS rejected the turns of "${scenario.id}": ${response.status} ${await response.text()}`
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

  /** Which turns are still queued, and which were consumed. */
  async state(): Promise<unknown> {
    const response = await fetch(`${this.endpoint}/__control/state`);
    return await response.json();
  }

  stop(): void {
    this.proc.kill();
  }
}

export interface KrsLoadReport {
  queued: number;
}

interface KrsState {
  queuedTurns?: string[];
  calls?: number;
  unmatchedCalls?: number;
  controlPlaneCalls?: string[];
  unknownTargets?: string[];
}

/**
 * Names of turns written to answer an unpredictable number of calls, which are
 * meant to outlive the run and so are not evidence of an unreached exchange.
 */
function stickyTurnNames(turns: readonly unknown[]): Set<string> {
  const names = new Set<string>();
  for (const turn of turns) {
    if (!turn || typeof turn !== 'object') continue;
    const { name, times } = turn as { name?: unknown; times?: unknown };
    if (times === 0 && typeof name === 'string') names.add(name);
  }
  return names;
}

/**
 * Where the exchange is written: the run's output directory when it has one,
 * since that is what CI collects, and beside the TUI log otherwise.
 */
export function exchangePath(
  scenario: Scenario,
  opts: RunOptions,
  tuiLogPath: string
): string {
  const fileName = `krs-exchange-${scenario.id}.json`;
  if (opts.outputDir) {
    mkdirSync(opts.outputDir, { recursive: true });
    return join(opts.outputDir, fileName);
  }
  return join(dirname(tuiLogPath), fileName);
}

/** Delegates to the live harness, and stops the fake KRS on cleanup. */
class KrsMockHarness implements TestHarness {
  private readonly stickyTurnNames: Set<string>;

  constructor(
    private readonly inner: LiveHarness,
    private readonly server: MockKrsProcess,
    private readonly exchangePath: string,
    turns: readonly unknown[]
  ) {
    this.stickyTurnNames = stickyTurnNames(turns);
  }

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

  async selfChecks(): Promise<VerifyResult[]> {
    const state = (await this.server.state()) as KrsState;
    const results: VerifyResult[] = [];

    // A turn nothing asked for means the scenario never reached the exchange it
    // scripted, which asserting the screen alone cannot catch.
    const leftover = (state.queuedTurns ?? []).filter(
      (name) => !this.stickyTurnNames.has(name)
    );
    results.push({
      predicate: 'krs.turnsConsumed',
      passed: leftover.length === 0,
      ...(leftover.length > 0
        ? { actual: `turns never requested: ${leftover.join(', ')}` }
        : {}),
    });

    results.push({
      predicate: 'krs.everyCallScripted',
      passed: (state.unmatchedCalls ?? 0) === 0,
      ...(state.unmatchedCalls
        ? { actual: `${state.unmatchedCalls} call(s) had no matching turn` }
        : {}),
    });

    // KAS reaches more than KRS, and what it reaches changes between versions.
    // Not an assertion: it calls things it tolerates a refusal for (InvokeMCP,
    // GetFeatureConfiguration) and completes the turn anyway, so failing on them
    // would fail a working run. Recorded and printed instead, so when an
    // uncovered call *does* break a turn the operation is named right here rather
    // than inferred from a screen that never changed.
    const unknown = [...new Set(state.unknownTargets ?? [])];
    if (unknown.length > 0) {
      console.log(
        `KRS: KAS called operation(s) this fixture does not implement: ${unknown.join(', ')}`
      );
    }

    return results;
  }

  async cleanup(): Promise<void> {
    // Written before the server dies: without it a failure says only that the
    // screen never changed, which cannot distinguish an unmatched turn from a
    // call KAS never made.
    await this.writeExchange();
    try {
      await this.inner.cleanup();
    } finally {
      // Always release the port, even if the TUI teardown throws.
      this.server.stop();
    }
  }

  private async writeExchange(): Promise<void> {
    try {
      const [state, requests] = await Promise.all([
        this.server.state(),
        this.server.requests(),
      ]);
      writeFileSync(
        this.exchangePath,
        JSON.stringify({ state, requests }, null, 2)
      );
      console.log(`KRS exchange: ${this.exchangePath}`);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      console.log(`KRS exchange unavailable: ${message}`);
    }
  }
}

/** Where a scenario's `{{fixture:NAME}}` resolves to. */
const FIXTURES_DIR = join(import.meta.dir, '../../fixtures');

/**
 * An agent config carrying a scenario's MCP servers, or null when it declares
 * none. The agent is selected by name at launch, so the servers it names are
 * the only ones the engine spawns.
 */
function mcpAgentConfig(
  scenario: Scenario
): { name: string; filePath: string; contents: string } | null {
  const servers = scenario.mcpServers;
  if (!servers || Object.keys(servers).length === 0) return null;

  const resolved = Object.fromEntries(
    Object.entries(servers).map(([name, spec]) => [
      name,
      {
        ...spec,
        command: resolveFixtures(spec.command),
        ...(spec.args ? { args: spec.args.map(resolveFixtures) } : {}),
      },
    ])
  );
  const name = 'scenario_mcp';
  return {
    name,
    filePath: `.kiro/agents/${name}.json`,
    contents: JSON.stringify(
      {
        name,
        description: 'Agent carrying the scenario-declared MCP servers.',
        prompt: 'You are a test agent.',
        tools: ['*'],
        mcpServers: resolved,
      },
      null,
      2
    ),
  };
}

function resolveFixtures(value: string): string {
  return value.replace(/\{\{fixture:([^}]+)\}\}/g, (_, file: string) =>
    join(FIXTURES_DIR, file)
  );
}

export function createKrsMockBackend(engine: Engine = 'kas'): ScenarioBackend {
  if (engine !== 'kas') {
    // Only the KAS engine talks to KRS; v2 has its own client and its own
    // endpoint override.
    throw new Error(
      `backend "krs-mock" only supports engine "kas", got "${engine}"`
    );
  }

  return {
    id: 'krs-mock',
    engine,
    async launch(scenario: Scenario, opts: RunOptions): Promise<TestHarness> {
      // Checked before anything is spawned: a scenario with nothing scripted
      // would otherwise fail on its first prompt, several steps from the cause.
      if (!scenario.turns || scenario.turns.length === 0) {
        throw new Error(
          `scenario "${scenario.id}" has no KRS turns, so it cannot run against the fake KRS`
        );
      }

      const server = await MockKrsProcess.start();
      try {
        await server.loadTurns(scenario);

        // A scenario's MCP servers ride on an agent config, which is where the
        // engine reads them from. Absent, nothing is planted and the registry
        // stays empty.
        const mcpAgent = mcpAgentConfig(scenario);

        const builder = E2ETestCase.builder()
          .withTestName(`scenario-${scenario.id}-krs-mock-${Date.now()}`)
          // A scenario's own size wins over the lane default: its assertions
          // encode a layout that only holds at that width.
          .withTerminal(
            scenario.terminal ?? opts.terminal ?? { width: 120, height: 40 }
          )
          .withTimeout(opts.timeout ?? scenario.timeout ?? 120_000)
          .withKasEngine()
          // A resolvable user-level subagent, so a delegation scenario can
          // target a real agent (KAS reads ~/.kiro/agents) instead of an empty
          // registry. Additive: scenarios that name another agent are
          // unaffected.
          .withPrelaunchFile(
            '.kiro/agents/test_subagent.json',
            JSON.stringify({
              name: 'test_subagent',
              description: 'Resolvable test subagent for delegation scenarios.',
              prompt: 'You are a test subagent. Complete the delegated task.',
              tools: [],
            })
          )
          .withEnv({
            // A scenario's own env first, so the wiring below always wins.
            ...(scenario.env ?? {}),
            KIRO_AGENT_ENGINE: 'kas',
            // Appended by kas.ts as KAS's `--endpoint`.
            KIRO_KAS_ENDPOINT: server.endpoint,
            // Appended by kas.ts as KAS's `--control-plane-endpoint`. KAS gates
            // every prompt on a model registry it fetches from the control
            // plane, which `--endpoint` does not cover; without this the call
            // escapes to the real service and fails closed on the fake key
            // below. Pointing it here also means any control-plane operation the
            // fixture has not implemented is refused by this server, under its
            // own name, instead of timing out against production.
            KIRO_KAS_CONTROL_PLANE_ENDPOINT: server.endpoint,
            // Read by KAS ahead of every auth mode, so no login is needed.
            KIRO_API_KEY: server.apiKey,
          });

        if (mcpAgent) {
          builder
            .withPrelaunchFile(mcpAgent.filePath, mcpAgent.contents)
            .withCliArgs('--agent', mcpAgent.name);
        }

        const testCase = await builder.launch();

        return new KrsMockHarness(
          new LiveHarness(testCase),
          server,
          exchangePath(scenario, opts, testCase.getTuiLogPath()),
          scenario.turns
        );
      } catch (error) {
        server.stop();
        throw error;
      }
    },
  };
}
