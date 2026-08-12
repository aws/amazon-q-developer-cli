import * as fs from 'node:fs';
import * as path from 'node:path';
import type {
  PromptRequest,
  PromptResponse,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from '../../../../src/test-utils/acp-mock/AcpTestCase';
import type { MockStreamItem } from '../../../types/chat-cli';
import type {
  AcpRecordedServerMessage,
  AcpWireScenarioFixture,
  AcpWireTurnFixture,
} from '../../acp-wire-fixtures';
import { computePromptContractHash } from '../../acp-wire-fixtures';
import type { Scenario } from '../../types';
import type { AcpMockScenarioConfig } from './config';

const DEFAULT_FIXTURE_DIR = path.join(
  __dirname,
  '../../../smoke/fixtures/acp-wire'
);
const LEGACY_FIXTURE_DIR = path.join(__dirname, '../../../smoke/fixtures');
const NOTIFICATION_DELAY_MS = 20;

interface LegacyPromptFixture {
  stepIndex: number;
  promptText: string;
  events: MockStreamItem[];
}

interface LegacyScenarioFixture {
  schemaVersion: 1;
  scenarioId: string;
  contractHash: string;
  prompts: LegacyPromptFixture[];
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function promptSteps(scenario: Scenario): AcpWireTurnFixture[] {
  return scenario.steps
    .map((step, index) => ({ step, index }))
    .filter(({ step }) => step.startsWith('prompt:'))
    .map(({ step, index }) => ({
      stepIndex: index,
      promptText: step.substring('prompt:'.length),
      serverMessages: [],
    }));
}

function loadJson(filePath: string): unknown | null {
  if (!fs.existsSync(filePath)) return null;
  try {
    return JSON.parse(fs.readFileSync(filePath, 'utf-8'));
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    throw new Error(`Failed to parse fixture "${filePath}": ${message}`, {
      cause: err,
    });
  }
}

function shouldUseLegacyFallback(fixturesDir?: string): boolean {
  if (!fixturesDir) return true;
  return path.resolve(fixturesDir) === path.resolve(DEFAULT_FIXTURE_DIR);
}

function resolveFixtureRef(
  scenario: Scenario,
  scenarioConfig?: AcpMockScenarioConfig
): string {
  return scenarioConfig?.fixture?.ref ?? scenario.id;
}

function parseToolInput(input?: string): unknown {
  if (!input) return {};
  try {
    return JSON.parse(input);
  } catch {
    return input;
  }
}

function inferToolKind(name: string, rawInput: unknown): 'read' | 'edit' | 'execute' {
  const normalized = name.toLowerCase();
  if (normalized.includes('read')) return 'read';
  if (
    normalized.includes('write') ||
    normalized.includes('replace') ||
    normalized.includes('edit')
  ) {
    return 'edit';
  }
  if (
    rawInput &&
    typeof rawInput === 'object' &&
    'command' in (rawInput as Record<string, unknown>)
  ) {
    return 'execute';
  }
  return 'execute';
}

function convertLegacyFixture(
  fixture: LegacyScenarioFixture,
  sessionId: string
): AcpWireTurnFixture[] {
  return fixture.prompts.map((prompt) => {
    let promptResponse: AcpWireTurnFixture['promptResponse'];
    const serverMessages = prompt.events.flatMap<AcpRecordedServerMessage>(
      (event) => {
        if (event.kind !== 'event') return [];
        switch (event.data.kind) {
          case 'AssistantResponseEvent':
            return [
              {
                kind: 'notification',
                method: 'session/update',
                params: {
                  sessionId,
                  update: {
                    sessionUpdate: 'agent_message_chunk',
                    content: { type: 'text', text: event.data.data.content },
                  },
                },
              },
            ];
          case 'ReasoningEvent':
            return [
              {
                kind: 'notification',
                method: 'session/update',
                params: {
                  sessionId,
                  update: {
                    sessionUpdate: 'agent_thought_chunk',
                    content: { type: 'text', text: event.data.data.text ?? '' },
                  },
                },
              },
            ];
          case 'ToolUseEvent': {
            const rawInput = parseToolInput(event.data.data.input);
            return [
              {
                kind: 'notification',
                method: 'session/update',
                params: {
                  sessionId,
                  update: {
                    sessionUpdate: 'tool_call',
                    toolCallId: event.data.data.tool_use_id,
                    title: event.data.data.name,
                    kind: inferToolKind(event.data.data.name, rawInput),
                    rawInput,
                  },
                },
              },
            ];
          }
          case 'MetadataEvent':
            if (event.data.data.stop_reason) {
              promptResponse = {
                result: { stopReason: event.data.data.stop_reason },
              };
              return [
                {
                  kind: 'notification',
                  method: 'session/update',
                  params: {
                    sessionId,
                    update: {
                      sessionUpdate: 'session_info_update',
                      _meta: { kiro: { kind: 'turn_completion' } },
                    },
                  },
                },
              ];
            }
            return [];
          default:
            return [];
        }
      }
    );

    return {
      stepIndex: prompt.stepIndex,
      promptText: prompt.promptText,
      serverMessages,
      promptResponse,
    };
  });
}

async function replayServerMessage(
  testCase: AcpTestCase,
  message: AcpRecordedServerMessage,
  sessionId: string
): Promise<void> {
  const params =
    message.params &&
    typeof message.params === 'object' &&
    'sessionId' in (message.params as Record<string, unknown>)
      ? {
          ...(message.params as Record<string, unknown>),
          sessionId,
        }
      : message.params;

  if (message.kind === 'notification') {
    testCase.mock.notify(message.method, params);
    await delay(NOTIFICATION_DELAY_MS);
    return;
  }

  await testCase.mock.request(message.method, params);
  await delay(NOTIFICATION_DELAY_MS);
}

export function parseAcpMockFixture(
  scenario: Scenario,
  scenarioConfig?: AcpMockScenarioConfig,
  fixturesDir?: string,
  sessionId = `scenario-${scenario.id}`
): AcpWireTurnFixture[] {
  const promptFixtures = promptSteps(scenario);
  if (promptFixtures.length === 0) return [];

  const fixtureRef = resolveFixtureRef(scenario, scenarioConfig);
  const fixturePath = path.join(
    fixturesDir ?? DEFAULT_FIXTURE_DIR,
    `${fixtureRef}.json`
  );
  const legacyPath = path.join(LEGACY_FIXTURE_DIR, `${fixtureRef}.json`);
  const raw =
    loadJson(fixturePath) ??
    (shouldUseLegacyFallback(fixturesDir) ? loadJson(legacyPath) : null);
  if (!raw) {
    throw new Error(
      `No fixture found for scenario "${scenario.id}" at ${fixturePath}`
    );
  }

  const contractHash = computePromptContractHash(scenario);
  if (
    typeof raw !== 'object' ||
    raw === null ||
    !('contractHash' in raw) ||
    typeof raw.contractHash !== 'string' ||
    raw.contractHash.length === 0
  ) {
    throw new Error(
      `Fixture missing contractHash for "${scenario.id}". Re-record before running the ACP mock backend.`
    );
  }
  if (raw.contractHash !== contractHash) {
    throw new Error(
      `Fixture stale: contractHash mismatch for "${scenario.id}". Re-record before running the ACP mock backend.`
    );
  }

  if ((raw as AcpWireScenarioFixture).schemaVersion === 2) {
    return (raw as AcpWireScenarioFixture).turns;
  }

  return convertLegacyFixture(raw as LegacyScenarioFixture, sessionId);
}

export function registerPromptTurnFixtureReplay(
  testCase: AcpTestCase,
  scenario: Scenario,
  scenarioConfig: AcpMockScenarioConfig | undefined,
  sessionId: string,
  fixturesDir?: string
): void {
  const turns = parseAcpMockFixture(
    scenario,
    scenarioConfig,
    fixturesDir,
    sessionId
  );
  if (turns.length === 0) return;

  let promptIndex = 0;
  testCase.mock.on<PromptRequest, PromptResponse>(
    'session/prompt',
    async (params: PromptRequest) => {
      const turn = turns[promptIndex];
      promptIndex++;
      if (!turn) {
        throw new Error(
          `Fixture prompt overrun for "${scenario.id}". Re-record before running the ACP mock backend.`
        );
      }

      const promptBlocks = (
        params as { prompt?: Array<{ type?: string; text?: string }> }
      ).prompt ?? [];
      const actualPrompt = promptBlocks
        .filter((block) => block.type === 'text')
        .map((block) => block.text ?? '')
        .join('');
      if (turn.promptText && actualPrompt && turn.promptText !== actualPrompt) {
        throw new Error(
          `Prompt fixture mismatch for "${scenario.id}": expected "${turn.promptText}", got "${actualPrompt}"`
        );
      }

      for (const message of turn.serverMessages) {
        await replayServerMessage(testCase, message, sessionId);
      }

      if (turn.promptResponse?.error) {
        throw new Error(turn.promptResponse.error.message);
      }
      if (turn.promptResponse?.result) {
        return turn.promptResponse.result as PromptResponse;
      }

      return new Promise<PromptResponse>(() => {});
    }
  );
}
