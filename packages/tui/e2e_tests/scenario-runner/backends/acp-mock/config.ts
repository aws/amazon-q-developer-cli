import type { Scenario } from '../../types';

export interface AcpMockFixtureConfig {
  mode: 'acp-wire';
  ref?: string;
}

export interface KasAcpMockScenarioConfig {
  ext?: Record<string, unknown>;
  session?: Record<string, unknown>;
  local?: Record<string, unknown>;
  upstream?: Record<string, unknown>;
}

export interface V2AcpMockScenarioConfig {
  options?: Record<string, unknown>;
}

export interface AcpMockScenarioConfig {
  fixture?: AcpMockFixtureConfig;
  kas?: KasAcpMockScenarioConfig;
  v2?: V2AcpMockScenarioConfig;
}

export function getAcpMockScenarioConfig(
  scenario: Scenario
): AcpMockScenarioConfig | undefined {
  return scenario.acpMock as AcpMockScenarioConfig | undefined;
}
