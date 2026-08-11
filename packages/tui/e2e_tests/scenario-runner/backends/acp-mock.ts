import * as fs from 'node:fs';
import * as path from 'node:path';
import type {
  PromptRequest,
  PromptResponse,
} from '@agentclientprotocol/sdk';
import { AcpTestCase } from '../../../acp_integ_tests/shared/AcpTestCase';
import { installStatefulKas } from '../../../acp_integ_tests/shared/sticky-defaults-harness';
import type { MockStreamItem } from '../../types/chat-cli';
import type {
  AcpRecordedServerMessage,
  AcpWireScenarioFixture,
  AcpWireTurnFixture,
} from '../acp-wire-fixtures';
import { computePromptContractHash } from '../acp-wire-fixtures';
import type {
  RunOptions,
  Scenario,
  ScenarioBackend,
  TestHarness,
} from '../types';

const DEFAULT_FIXTURE_DIR = path.join(
  __dirname,
  '../../smoke/fixtures/acp-wire'
);
const LEGACY_FIXTURE_DIR = path.join(__dirname, '../../smoke/fixtures');
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

class AcpMockHarness implements TestHarness {
  constructor(private readonly testCase: AcpTestCase) {}

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

  cleanup(): Promise<void> {
    return this.testCase.cleanup();
  }
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

function parseFixture(
  scenario: Scenario,
  fixturesDir?: string,
  sessionId = `scenario-${scenario.id}`
): AcpWireTurnFixture[] {
  const promptFixtures = promptSteps(scenario);
  if (promptFixtures.length === 0) return [];

  const fixturePath = path.join(fixturesDir ?? DEFAULT_FIXTURE_DIR, `${scenario.id}.json`);
  const legacyPath = path.join(LEGACY_FIXTURE_DIR, `${scenario.id}.json`);
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
  if (normalized.includes('write') || normalized.includes('replace') || normalized.includes('edit')) {
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
    const serverMessages = prompt.events.flatMap<AcpRecordedServerMessage>((event) => {
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
    });

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

function registerCommonHandlers(
  testCase: AcpTestCase,
  sessionId: string
): { emitInitialNotifications: () => Promise<void> } {
  const toolTags = [
    {
      source: 'builtin',
      tag: 'read',
      description: 'built-in file and search tools',
    },
    {
      source: 'builtin',
      tag: 'write',
      description: 'built-in file editing tools',
    },
    {
      source: 'builtin',
      tag: 'shell',
      description: 'built-in shell execution tools',
    },
    {
      source: 'builtin',
      tag: 'subagent',
      description: 'built-in subagent orchestration tools',
    },
    {
      source: 'mcp',
      tag: '@git/status',
      description: 'Git status from MCP',
    },
  ] as const;

  installStatefulKas(testCase, {
    sessionId,
    models: [
      { value: 'claude-sonnet-4', name: 'Claude Sonnet 4' },
      { value: 'claude-opus-4.7', name: 'Claude Opus 4.7' },
    ],
    initialModel: 'claude-sonnet-4',
    initialEffort: 'high',
    agents: [
      { value: 'vibe', name: 'Default' },
      { value: 'plan', name: 'Plan' },
      { value: 'kiro_guide', name: 'Guide' },
    ],
    initialMode: 'vibe',
    modeModelMap: {
      vibe: 'claude-sonnet-4',
      plan: 'claude-sonnet-4',
      kiro_guide: 'claude-sonnet-4',
    },
  });
  testCase.mock.on('session/set_mode', () => ({}));
  testCase.mock.on('session/cancel', () => ({}));
  testCase.mock.on('session/list', () => ({ sessions: [] }));
  testCase.mock.on('session/fork', () => ({ sessionId: `${sessionId}-fork` }));
  testCase.mock.on('_kiro/help', () => ({
    commands: [
      { name: '/<COMMAND>', description: 'Command placeholder' },
      { name: '/help', description: 'Open help' },
      { name: '/context', description: 'Manage context' },
    ],
  }));
  testCase.mock.on('_kiro/account/getUsage', () => ({
    success: true,
    message: 'Usage loaded',
    data: {
      planName: 'Test Plan',
      billingCycleReset: '2026-09-01',
      overagesEnabled: true,
      isEnterprise: false,
      usageBreakdowns: [
        {
          displayName: 'Credits',
          used: 1,
          limit: 42,
          percentage: 2.4,
          currentOverages: 0,
          overageRate: 0,
          overageCharges: 0,
          currency: 'USD',
          hasLimit: true,
        },
      ],
      bonusCredits: [],
      addOnCredits: [],
      overageCapable: true,
    },
  }));
  testCase.mock.on('_kiro/hooks/list', () => ({ hooks: [] }));
  testCase.mock.on('_kiro/knowledge', () => ({
    entries: [],
    message: 'No knowledge entries',
  }));
  testCase.mock.on('_kiro/session/context', (params: Record<string, unknown>) => {
    const subcommand = (params as { subcommand?: string }).subcommand;
    if (subcommand === 'show') {
      return {
        entries: [],
        breakdown: {
          contextFiles: {
            percent: 22,
            tokens: 220,
            items: [
              {
                name: 'packages/tui/package.json',
                tokens: 140,
                matched: true,
                percent: 14,
              },
              {
                name: 'packages/tui/src/index.tsx',
                tokens: 80,
                matched: true,
                percent: 8,
              },
            ],
          },
          sessionFiles: {
            percent: 8,
            tokens: 80,
            items: [
              {
                name: 'docs/design/testing-certification/scenario-runner-v2-architecture.md',
                tokens: 80,
                matched: true,
                percent: 8,
              },
            ],
          },
          tools: {
            percent: 10,
            tokens: 100,
            groups: [
              {
                name: 'Built-in',
                source: 'builtin',
                tokens: 100,
                percent: 10,
                items: [
                  { name: 'read', tokens: 60, percent: 6 },
                  { name: 'shell', tokens: 40, percent: 4 },
                ],
              },
            ],
          },
          kiroResponses: {
            percent: 35,
            tokens: 350,
          },
          yourPrompts: {
            percent: 25,
            tokens: 250,
          },
        },
      };
    }
    return { success: true, message: 'context updated' };
  });
  testCase.mock.on('_kiro/session/compact', async () => {
    testCase.mock.notify('session/update', {
      sessionId,
      update: {
        sessionUpdate: 'session_info_update',
        _meta: { kiro: { kind: 'summarization_started' } },
      },
    });
    await delay(NOTIFICATION_DELAY_MS);
    testCase.mock.notify('session/update', {
      sessionId,
      update: {
        sessionUpdate: 'session_info_update',
        _meta: {
          kiro: {
            kind: 'summarization_failed',
            error: 'Conversation too short to compact',
          },
        },
      },
    });
    return { success: true };
  });
  testCase.mock.on('_kiro/codeIntelligence', (params: Record<string, unknown>) => {
    const subcommand = (params as { subcommand?: string }).subcommand;
    if (subcommand === 'overview') {
      return { success: true, overview: 'overview' };
    }
    return {
      success: true,
      status: {
        initialized: true,
        languages: ['TypeScript'],
        lspServers: [
          {
            name: 'typescript-language-server',
            languages: ['TypeScript'],
            status: 'running',
            isAvailable: true,
          },
        ],
      },
    };
  });
  testCase.mock.on('_kiro/mcp/resetServer', () => ({}));

  return {
    async emitInitialNotifications() {
      testCase.mock.notify('_kiro/tools/didChange', {
        sessionId,
        tags: toolTags,
      });
      await delay(NOTIFICATION_DELAY_MS);
    },
  };
}

export function createAcpMockBackend(): ScenarioBackend {
  return {
    id: 'acp-mock',
    engine: 'kas',
    async launch(scenario: Scenario, opts: RunOptions): Promise<TestHarness> {
      const sessionId = `scenario-${scenario.id}`;
      const turns = parseFixture(scenario, opts.fixturesDir, sessionId);
      let promptIndex = 0;

      const testCase = new AcpTestCase({
        testName: `scenario-${scenario.id}-acp-mock-${Date.now()}`,
      });
      const common = registerCommonHandlers(testCase, sessionId);
      testCase.mock.on<PromptRequest, PromptResponse>('session/prompt', async (params: PromptRequest) => {
        const turn = turns[promptIndex];
        promptIndex++;
        if (!turn) {
          throw new Error(
            `Fixture prompt overrun for "${scenario.id}". Re-record before running the ACP mock backend.`
          );
        }

        const promptBlocks = (params as { prompt?: Array<{ type?: string; text?: string }> }).prompt ?? [];
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
      });

      await testCase.launch();
      await testCase.mock.awaitConnection();
      await testCase.waitForVisibleText('ask a question', 10_000);
      await common.emitInitialNotifications();
      return new AcpMockHarness(testCase);
    },
  };
}
