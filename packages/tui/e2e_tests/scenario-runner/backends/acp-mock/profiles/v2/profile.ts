import type { AcpTestCase } from '../../../../../../src/test-utils/acp-mock/AcpTestCase';
import type { Scenario } from '../../../../types';
import type { AcpMockScenarioConfig } from '../../config';
import type { AcpMockProfileInstallResult } from '../../profile';

export function installV2AcpMockScenario(
  _testCase: AcpTestCase,
  scenario: Scenario,
  _opts: {
    sessionId: string;
    fixturesDir?: string;
    scenarioConfig?: AcpMockScenarioConfig;
  }
): AcpMockProfileInstallResult {
  throw new Error(
    `ACP mock profile for engine "v2" is not implemented yet for scenario "${scenario.id}".`
  );
}
