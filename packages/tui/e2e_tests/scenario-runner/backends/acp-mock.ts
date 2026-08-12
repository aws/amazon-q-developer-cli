import { AcpTestCase } from '../../../src/test-utils/acp-mock/AcpTestCase';
import type {
  Engine,
  RunOptions,
  Scenario,
  ScenarioBackend,
  TestHarness,
} from '../types';
import { getAcpMockScenarioConfig } from './acp-mock/config';
import {
  buildEffortConfigOptions,
  buildModelConfigOptions,
} from './acp-mock/profiles/kas/handlers';
import { installLocalSeam } from './acp-mock/local-seam';
import type {
  AcpMockProfileInstaller,
  SupportedAcpMockProfile,
} from './acp-mock/profile';
import { installKasAcpMockScenario } from './acp-mock/profiles/kas/profile';
import { installV2AcpMockScenario } from './acp-mock/profiles/v2/profile';

class AcpMockHarness implements TestHarness {
  constructor(
    private readonly testCase: AcpTestCase,
    private readonly cleanupHooks: Array<() => void> = []
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
    return this.testCase.waitForVisibleText(text, timeout);
  }

  waitForIdle(timeout?: number): Promise<void> {
    return this.testCase
      .waitForStore(
        (state: { isProcessing: boolean }) => !state.isProcessing,
        timeout
      )
      .then(() => {});
  }

  expectExit(timeout?: number): Promise<number> {
    return this.testCase.expectExit(timeout);
  }

  async cleanup(): Promise<void> {
    await this.testCase.cleanup();
    for (const cleanupHook of this.cleanupHooks) {
      cleanupHook();
    }
  }
}

const ACP_MOCK_PROFILES: Record<
  SupportedAcpMockProfile,
  AcpMockProfileInstaller['install']
> = {
  kas: installKasAcpMockScenario,
  v2: installV2AcpMockScenario,
};

function sanitizeTestNameSegment(value: string): string {
  return value.replace(/[^a-zA-Z0-9._-]+/g, '-');
}

export function buildAcpMockTestName(
  scenarioId: string,
  engine: Engine,
  now: number = Date.now()
): string {
  return [
    'scenario',
    sanitizeTestNameSegment(scenarioId),
    'acp-mock',
    sanitizeTestNameSegment(engine),
    String(now),
  ].join('-');
}

export function createAcpMockBackend(engine: Engine = 'kas'): ScenarioBackend {
  return {
    id: 'acp-mock',
    engine,
    async launch(scenario: Scenario, opts: RunOptions): Promise<TestHarness> {
      const sessionId = `scenario-${scenario.id}`;
      const scenarioConfig = getAcpMockScenarioConfig(scenario);
      const kasConfig =
        opts.backend.engine === 'kas' ? scenarioConfig?.kas : undefined;
      const testCaseOptions: ConstructorParameters<typeof AcpTestCase>[0] = {
        testName: buildAcpMockTestName(scenario.id, opts.backend.engine),
      };
      const localSeam = installLocalSeam(testCaseOptions, kasConfig);
      const testCase = new AcpTestCase(testCaseOptions);
      try {
        const installProfile = ACP_MOCK_PROFILES[opts.backend.engine];
        if (!installProfile) {
          throw new Error(
            `Unsupported ACP mock profile engine: ${opts.backend.engine}`
          );
        }
        const artifacts = installProfile(testCase, scenario, {
          sessionId,
          fixturesDir: opts.fixturesDir,
          scenarioConfig,
        });

        await testCase.launch();
        await testCase.mock.awaitConnection();
        await testCase.waitForVisibleText('ask a question', 10_000);
        await artifacts.emitInitialNotifications();

        return new AcpMockHarness(testCase, [
          artifacts.cleanup,
          () => localSeam?.cleanup(),
        ]);
      } catch (err) {
        localSeam?.cleanup();
        await testCase.cleanup().catch(() => {});
        throw err;
      }
    },
  };
}

export const kasAcpMockTestHelpers = {
  buildEffortConfigOptions,
  buildModelConfigOptions,
};
