import { afterEach, describe, expect, it } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { buildAcpMockTestName } from './backends/acp-mock';
import type { AcpMockScenarioConfig } from './backends/acp-mock/config';
import { parseAcpMockFixture } from './backends/acp-mock/fixture-replay';
import {
  buildModelConfigOptions,
  registerKasExtHandlers,
  registerKasSessionHandlers,
} from './backends/acp-mock/profiles/kas/handlers';
import { installKasAcpMockScenario } from './backends/acp-mock/profiles/kas/profile';
import { installV2AcpMockScenario } from './backends/acp-mock/profiles/v2/profile';
import { computePromptContractHash } from './acp-wire-fixtures';
import type { Scenario } from './types';

type RpcHandler = (params?: unknown) => unknown | Promise<unknown>;

class FakeMockServer {
  handlers = new Map<string, RpcHandler>();
  notifications: Array<{ method: string; params: unknown }> = [];
  requests: Array<{ method: string; params: unknown }> = [];

  on(method: string, handler: RpcHandler): this {
    this.handlers.set(method, handler);
    return this;
  }

  notify(method: string, params: unknown): void {
    this.notifications.push({ method, params });
  }

  async request(method: string, params: unknown): Promise<unknown> {
    this.requests.push({ method, params });
    return {};
  }
}

function fakeTestCase() {
  return {
    mock: new FakeMockServer(),
  } as const;
}

function baseScenario(id: string, steps: string[]): Scenario {
  return {
    id,
    name: id,
    category: 'slash-commands',
    description: id,
    steps,
    verify: ['screen.contains:placeholder'],
  };
}

type AcpMockScenario = Scenario & { acpMock?: AcpMockScenarioConfig };

async function invoke(
  mock: FakeMockServer,
  method: string,
  params?: unknown
): Promise<unknown> {
  const handler = mock.handlers.get(method);
  if (!handler) {
    throw new Error(`Missing handler for ${method}`);
  }
  return await handler(params);
}

describe('ACP mock profiles', () => {
  const tempDirs: string[] = [];

  afterEach(() => {
    for (const dir of tempDirs.splice(0)) {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('registers the default KAS ext handlers', async () => {
    const tc = fakeTestCase();
    const installed = registerKasExtHandlers(tc as never, 'session-1', undefined);

    expect(tc.mock.handlers.has('_kiro/help')).toBe(true);
    expect(tc.mock.handlers.has('_kiro/account/getUsage')).toBe(true);
    expect(tc.mock.handlers.has('_kiro/knowledge')).toBe(true);
    expect(tc.mock.handlers.has('_kiro/session/context')).toBe(true);
    expect(tc.mock.handlers.has('_kiro/session/compact')).toBe(true);
    expect(tc.mock.handlers.has('_kiro/codeIntelligence')).toBe(true);
    expect(tc.mock.handlers.has('_kiro/mcp/resetServer')).toBe(true);

    const help = (await invoke(tc.mock, '_kiro/help')) as {
      commands: Array<{ name: string }>;
    };
    expect(help.commands[0]?.name).toBe('/<COMMAND>');

    await installed.emitInitialNotifications();
    expect(tc.mock.notifications[0]?.method).toBe('_kiro/tools/didChange');
  });

  it('allows KAS ext handler overrides by scenario config', async () => {
    const tc = fakeTestCase();
    registerKasExtHandlers(tc as never, 'session-1', {
      help: {
        commands: [{ name: '/override', description: 'Scenario Override' }],
      },
    });

    const help = (await invoke(tc.mock, '_kiro/help')) as {
      commands: Array<{ name: string; description: string }>;
    };
    expect(help.commands).toEqual([
      { name: '/override', description: 'Scenario Override' },
    ]);
  });

  it('allows KAS session handler overrides by config id', async () => {
    const tc = fakeTestCase();
    registerKasSessionHandlers(tc as never, 'session-1', {
      set_config_option: {
        model: {
          configOptions: buildModelConfigOptions('claude-opus-4.7'),
        },
      },
    });

    const result = (await invoke(tc.mock, 'session/set_config_option', {
      configId: 'model',
      value: 'claude-opus-4.7',
    })) as { configOptions: Array<unknown> };

    expect(result.configOptions).toHaveLength(3);
  });

  it('installs fixture replay alongside the KAS handlers', async () => {
    const fixturesDir = mkdtempSync(join(tmpdir(), 'kiro-acp-fixtures-'));
    tempDirs.push(fixturesDir);
    const scenario = baseScenario('fixture-and-help', ['prompt:hello fixture']);
    writeFileSync(
      join(fixturesDir, `${scenario.id}.json`),
      JSON.stringify({
        schemaVersion: 2,
        scenarioId: scenario.id,
        contractHash: computePromptContractHash(scenario),
        turns: [
          {
            stepIndex: 0,
            promptText: 'hello fixture',
            serverMessages: [
              {
                kind: 'notification',
                method: 'session/update',
                params: {
                  sessionId: 'fixture-session',
                  update: {
                    sessionUpdate: 'agent_message_chunk',
                    content: { type: 'text', text: 'Fixture hello' },
                  },
                },
              },
            ],
            promptResponse: { result: { stopReason: 'end_turn' } },
          },
        ],
      })
    );

    const tc = fakeTestCase();
    installKasAcpMockScenario(tc as never, scenario, {
      sessionId: 'session-1',
      fixturesDir,
      scenarioConfig: (scenario as AcpMockScenario).acpMock,
    });

    expect(tc.mock.handlers.has('_kiro/help')).toBe(true);
    expect(tc.mock.handlers.has('session/prompt')).toBe(true);

    const promptResult = await invoke(tc.mock, 'session/prompt', {
      prompt: [{ type: 'text', text: 'hello fixture' }],
    });
    expect(promptResult).toEqual({ stopReason: 'end_turn' });
    expect(tc.mock.notifications[0]?.method).toBe('session/update');
  });

  it('fails closed when the fixture is missing or stale', () => {
    const missingScenario = baseScenario('missing-fixture', ['prompt:missing']);
    const missingDir = mkdtempSync(join(tmpdir(), 'kiro-acp-missing-'));
    tempDirs.push(missingDir);

    expect(() =>
      parseAcpMockFixture(
        missingScenario,
        (missingScenario as AcpMockScenario).acpMock,
        missingDir
      )
    ).toThrow(/No fixture found/);

    const staleScenario = baseScenario('stale-fixture', ['prompt:stale']);
    const staleDir = mkdtempSync(join(tmpdir(), 'kiro-acp-stale-'));
    tempDirs.push(staleDir);
    writeFileSync(
      join(staleDir, `${staleScenario.id}.json`),
      JSON.stringify({
        schemaVersion: 2,
        scenarioId: staleScenario.id,
        contractHash: 'stale-contract',
        turns: [],
      })
    );

    expect(() =>
      parseAcpMockFixture(
        staleScenario,
        (staleScenario as AcpMockScenario).acpMock,
        staleDir
      )
    ).toThrow(/Fixture stale/);
  });

  it('keeps a V2 profile stub behind the same interface for fast follow work', () => {
    expect(() =>
      installV2AcpMockScenario(
        fakeTestCase() as never,
        baseScenario('v2', ['waitForText:ask a question']),
        {
          sessionId: 'session-1',
        }
      )
    ).toThrow(/engine "v2"/);
  });

  it('builds filesystem-safe test names for engine ids', () => {
    expect(buildAcpMockTestName('slash-help', 'kas', 1234)).toBe(
      'scenario-slash-help-acp-mock-kas-1234'
    );
  });
});
