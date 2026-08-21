import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';

import { E2ETestCase } from '../../E2ETestCase';
import type {
  Engine,
  RunOptions,
  Scenario,
  ScenarioBackend,
  TestHarness,
} from '../types';

/** Model config arrives asynchronously after the session opens. */
const MODEL_PIN_TIMEOUT_MS = 20_000;

/**
 * Seeded into a pinned run's workdir. A scenario that asks the model to read
 * this file can assert on the token, which reaches the screen only through a
 * tool call that actually returned content — no dependence on which tool the
 * model picks or on how the card is worded.
 */
const PROBE_FILE = 'probe.txt';
const PROBE_TOKEN = 'model-sanity-probe-token';

export class LiveHarness implements TestHarness {
  constructor(
    private readonly testCase: E2ETestCase,
    private readonly ownedWorkdir?: string
  ) {}

  sendKeys(input: string | number[]): Promise<void> {
    return this.testCase.sendKeys(input);
  }

  pressEnter(): Promise<void> {
    return this.testCase.pressEnter();
  }

  pressEscape(): Promise<void> {
    return this.testCase.pressEscape();
  }

  pressCtrlC(): Promise<void> {
    return this.testCase.pressCtrlC();
  }

  pressCtrlCTwice(): Promise<void> {
    return this.testCase.pressCtrlCTwice();
  }

  sleepMs(ms: number): Promise<void> {
    return this.testCase.sleepMs(ms);
  }

  getStore() {
    return this.testCase.getStore();
  }

  getSnapshot(): string[] {
    return this.testCase.getSnapshot();
  }

  getSnapshotHtml(): string {
    return this.testCase.getSnapshotHtml();
  }

  waitForText(text: string, timeout?: number): Promise<void> {
    return this.testCase.waitForText(text, timeout);
  }

  waitForIdle(timeout?: number): Promise<void> {
    return this.testCase.waitForIdle(timeout);
  }

  expectExit(timeout?: number): Promise<number> {
    return this.testCase.expectExit(timeout);
  }

  async cleanup(): Promise<void> {
    try {
      await this.testCase.cleanup();
    } finally {
      if (this.ownedWorkdir) {
        fs.rmSync(this.ownedWorkdir, { recursive: true, force: true });
      }
    }
  }
}

/**
 * The backend answers an unknown model id by quietly serving a different model,
 * and the id survives into the session record either way — so neither the run
 * passing nor the saved session shows which model was used. Both engines resolve
 * a current model only against the list the backend advertises, so a match
 * establishes that the backend names this id as the session's model. It does not
 * establish which weights produced the tokens.
 *
 * A miss has two causes worth telling apart: the backend resolved a different
 * model, or nothing resolved at all — which an unadvertised id and a session
 * that never authenticated both produce.
 */
async function assertModelPinned(
  testCase: E2ETestCase,
  model: string
): Promise<void> {
  try {
    await testCase.waitForStoreCondition(
      (s) => s.currentModel?.id === model,
      MODEL_PIN_TIMEOUT_MS
    );
  } catch {
    const actual = (await testCase.getStore()).currentModel;
    throw new Error(
      actual
        ? `model "${model}" is not the session's model; it is on "${actual.id}".`
        : `no model resolved within ${MODEL_PIN_TIMEOUT_MS}ms, so "${model}" is ` +
          `unconfirmed. An id the backend does not advertise never resolves ` +
          `(run /model to list them), and neither does a session that failed ` +
          `to authenticate — check the run's logs for an auth error first.`
    );
  }
}

export function createLiveBackend(engine: Engine = 'kas'): ScenarioBackend {
  return {
    id: 'live',
    engine,
    async launch(scenario: Scenario, opts: RunOptions): Promise<TestHarness> {
      // A pinned run gets its own directory: the probe file needs a known
      // location, and an empty tree keeps the tool-driving scenarios' results
      // the same on every machine.
      const workdir = opts.model
        ? fs.mkdtempSync(path.join(os.tmpdir(), `scenario-${scenario.id}-`))
        : undefined;
      if (workdir) {
        fs.writeFileSync(path.join(workdir, PROBE_FILE), `${PROBE_TOKEN}\n`);
      }

      let builder = E2ETestCase.builder()
        .withTestName(`scenario-${scenario.id}-${engine}-${Date.now()}`)
        // A scenario's own size wins over the lane default: its assertions
        // encode a layout that only holds at that width.
        .withTerminal(
          scenario.terminal ?? opts.terminal ?? { width: 120, height: 40 }
        )
        .withTimeout(opts.timeout ?? scenario.timeout ?? 120_000);

      if (workdir) {
        builder = builder.withCwd(workdir);
      }
      if (scenario.env) {
        builder = builder.withEnv(scenario.env);
      }
      // KAS runs its agent out of process, and a pinned run turns the mock API
      // off, which takes the IPC server with it. Neither leaves an agent
      // connection to wait for.
      if (engine === 'kas' || opts.model) {
        builder = builder.withoutAgentIpc();
      }
      // Pinned on both sides because each decides independently: the Rust
      // launcher reads the setting, the TUI reads the env var. Leaving either
      // implicit lets a host that already exports an engine (any run started
      // from inside a V3 session) silently answer for the lane, while the run
      // still reports whichever engine was asked for.
      builder = builder
        .withEnv({ KIRO_AGENT_ENGINE: engine === 'kas' ? 'kas' : 'v2' })
        .withGlobalSettings({
          'chat.agentEngine': engine === 'kas' ? 'v3' : 'v2',
        });
      if (opts.model) {
        // Passed as a flag rather than saved as `chat.defaultModel`, which sits
        // below an agent's own model in the precedence chain — an agent that
        // pins a model would otherwise decide what the lane tested.
        //
        // Test mode otherwise routes prompts through the mock API registry,
        // which answers an empty stream when nothing is queued, so a live turn
        // would wait on it forever.
        builder = builder
          .withCliArgs('--model', opts.model)
          .withEnv({ KIRO_TEST_LIVE_API: '1' });
      }

      const testCase = await builder.launch();
      const harness = new LiveHarness(testCase, workdir);
      if (opts.model) {
        // Tear down before rethrowing: the pin runs after launch, so the process
        // tree, sandbox and workdir all exist by the time it can fail.
        try {
          await assertModelPinned(testCase, opts.model);
        } catch (err) {
          await harness.cleanup().catch(() => {});
          throw err;
        }
      }
      return harness;
    },
  };
}
