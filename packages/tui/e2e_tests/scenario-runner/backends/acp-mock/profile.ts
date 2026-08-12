import type { AcpTestCase } from '../../../../src/test-utils/acp-mock/AcpTestCase';
import type { Engine, Scenario } from '../../types';
import type { AcpMockScenarioConfig } from './config';

export interface AcpMockProfileInstallResult {
  emitInitialNotifications: () => Promise<void>;
  cleanup: () => void;
}

export interface AcpMockProfileInstaller {
  install(
    testCase: AcpTestCase,
    scenario: Scenario,
    opts: {
      sessionId: string;
      fixturesDir?: string;
      scenarioConfig?: AcpMockScenarioConfig;
    }
  ): AcpMockProfileInstallResult;
}

export type SupportedAcpMockProfile = Engine;
