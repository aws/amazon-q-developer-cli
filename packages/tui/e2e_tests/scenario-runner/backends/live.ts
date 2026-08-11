import { E2ETestCase } from '../../E2ETestCase';
import type {
  Engine,
  RunOptions,
  Scenario,
  ScenarioBackend,
  TestHarness,
} from '../types';

class LiveHarness implements TestHarness {
  constructor(private readonly testCase: E2ETestCase) {}

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

  cleanup(): Promise<void> {
    return this.testCase.cleanup();
  }
}

export function createLiveBackend(engine: Engine = 'kas'): ScenarioBackend {
  return {
    id: 'live',
    engine,
    async launch(scenario: Scenario, opts: RunOptions): Promise<TestHarness> {
      let builder = E2ETestCase.builder()
        .withTestName(`scenario-${scenario.id}-${engine}-${Date.now()}`)
        .withTerminal(opts.terminal ?? { width: 120, height: 40 })
        .withTimeout(opts.timeout ?? scenario.timeout ?? 120_000);

      if (engine === 'kas') {
        builder = builder.withKasEngine().withEnv({ KIRO_AGENT_ENGINE: 'kas' });
      }

      const testCase = await builder.launch();
      return new LiveHarness(testCase);
    },
  };
}
