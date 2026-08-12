import type { AcpTestCase } from '../../../../../../src/test-utils/acp-mock/AcpTestCase';
import type { Scenario } from '../../../../types';
import type { AcpMockScenarioConfig } from '../../config';
import { registerPromptTurnFixtureReplay } from '../../fixture-replay';
import {
  registerKasExtHandlers,
  registerKasSessionHandlers,
} from './handlers';

export function installKasAcpMockScenario(
  testCase: AcpTestCase,
  scenario: Scenario,
  opts: {
    sessionId: string;
    fixturesDir?: string;
    scenarioConfig?: AcpMockScenarioConfig;
  }
): { emitInitialNotifications: () => Promise<void>; cleanup: () => void } {
  const { sessionId, fixturesDir, scenarioConfig } = opts;
  const kasConfig = scenarioConfig?.kas;
  registerKasSessionHandlers(testCase, sessionId, kasConfig?.session);
  const ext = registerKasExtHandlers(testCase, sessionId, kasConfig?.ext);
  registerPromptTurnFixtureReplay(
    testCase,
    scenario,
    scenarioConfig,
    sessionId,
    fixturesDir
  );
  return {
    emitInitialNotifications: ext.emitInitialNotifications,
    cleanup: () => {},
  };
}
