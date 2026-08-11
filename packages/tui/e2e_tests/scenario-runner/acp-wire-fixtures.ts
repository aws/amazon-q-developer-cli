import * as crypto from 'node:crypto';

export interface AcpRecordedNotification {
  kind: 'notification';
  method: string;
  params: unknown;
}

export interface AcpRecordedRequest {
  kind: 'request';
  method: string;
  params: unknown;
}

export type AcpRecordedServerMessage =
  | AcpRecordedNotification
  | AcpRecordedRequest;

export interface AcpPromptResponseFixture {
  result?: unknown;
  error?: {
    code: number;
    message: string;
    data?: unknown;
  };
}

export interface AcpWireTurnFixture {
  stepIndex: number;
  promptText: string;
  serverMessages: AcpRecordedServerMessage[];
  promptResponse?: AcpPromptResponseFixture;
  durationMs?: number;
}

export interface AcpWireScenarioFixture {
  schemaVersion: 2;
  scenarioId: string;
  backend?: string;
  engine?: string;
  contractHash?: string;
  turns: AcpWireTurnFixture[];
}

export function computePromptContractHash(scenario: {
  steps: string[];
}): string {
  const prompts = scenario.steps
    .filter((step) => step.startsWith('prompt:'))
    .map((step) => step.substring('prompt:'.length));

  return crypto
    .createHash('sha256')
    .update(JSON.stringify({ prompts }))
    .digest('hex');
}
