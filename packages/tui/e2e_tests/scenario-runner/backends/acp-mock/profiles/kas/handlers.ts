import { AcpTestCase } from '../../../../../../src/test-utils/acp-mock/AcpTestCase';
import {
  LEVELS,
  effortConfigOption,
  installStatefulKas,
  modeConfigOption,
  modelConfigOption,
} from '../../../../../../acp_integ_tests/shared/sticky-defaults-harness';

const NOTIFICATION_DELAY_MS = 20;

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function asRecord(value: unknown): Record<string, unknown> | null {
  return value && typeof value === 'object' && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function getRegistryOverride(
  overrides: Record<string, unknown> | undefined,
  key: string,
  method: string,
  selector?: string
): unknown {
  const override = overrides?.[key] ?? overrides?.[method];
  if (!selector) return override;

  const record = asRecord(override);
  if (!record) return override;
  if (selector in record) return record[selector];
  if ('default' in record) return record.default;
  return override;
}

export function registerKasExtHandlers(
  testCase: AcpTestCase,
  sessionId: string,
  overrides: Record<string, unknown> | undefined
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

  testCase.mock.on('_kiro/help', () => {
    return (
      getRegistryOverride(overrides, 'help', '_kiro/help') ?? {
        commands: [
          { name: '/<COMMAND>', description: 'Command placeholder' },
          { name: '/help', description: 'Open help' },
          { name: '/context', description: 'Manage context' },
        ],
      }
    );
  });
  testCase.mock.on('_kiro/account/getUsage', () => {
    return (
      getRegistryOverride(overrides, 'usage', '_kiro/account/getUsage') ?? {
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
      }
    );
  });
  testCase.mock.on('_kiro/hooks/list', () => {
    return getRegistryOverride(overrides, 'hooks.list', '_kiro/hooks/list') ?? {
      hooks: [],
    };
  });
  testCase.mock.on('_kiro/knowledge', () => {
    return getRegistryOverride(overrides, 'knowledge', '_kiro/knowledge') ?? {
      entries: [],
      message: 'No knowledge entries',
    };
  });
  testCase.mock.on(
    '_kiro/session/context',
    (params: Record<string, unknown>) => {
      const subcommand = (params as { subcommand?: string }).subcommand;
      const override = getRegistryOverride(
        overrides,
        'context',
        '_kiro/session/context',
        typeof subcommand === 'string' ? subcommand : undefined
      );
      if (override !== undefined) return override;
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
    }
  );
  testCase.mock.on('_kiro/session/compact', async () => {
    const override = getRegistryOverride(
      overrides,
      'compact',
      '_kiro/session/compact'
    );
    if (override !== undefined) return override;

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
  testCase.mock.on(
    '_kiro/codeIntelligence',
    (params: Record<string, unknown>) => {
      const subcommand = (params as { subcommand?: string }).subcommand;
      const override = getRegistryOverride(
        overrides,
        'code',
        '_kiro/codeIntelligence',
        typeof subcommand === 'string' ? subcommand : undefined
      );
      if (override !== undefined) return override;
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
    }
  );
  testCase.mock.on('_kiro/mcp/resetServer', () => {
    return (
      getRegistryOverride(overrides, 'mcp.reset', '_kiro/mcp/resetServer') ??
      {}
    );
  });

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

export function registerKasSessionHandlers(
  testCase: AcpTestCase,
  sessionId: string,
  overrides: Record<string, unknown> | undefined
): void {
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

  const newOverride = getRegistryOverride(overrides, 'new', 'session/new');
  if (newOverride !== undefined) {
    testCase.mock.on('session/new', () => newOverride);
  }

  const loadOverride = getRegistryOverride(overrides, 'load', 'session/load');
  if (loadOverride !== undefined) {
    testCase.mock.on('session/load', () => loadOverride);
  }

  const setConfigOverride = getRegistryOverride(
    overrides,
    'set_config_option',
    'session/set_config_option'
  );
  if (setConfigOverride !== undefined) {
    testCase.mock.on(
      'session/set_config_option',
      (params: Record<string, unknown>) => {
        const configId =
          typeof params.configId === 'string' ? params.configId : undefined;
        return (
          getRegistryOverride(
            overrides,
            'set_config_option',
            'session/set_config_option',
            configId
          ) ?? setConfigOverride
        );
      }
    );
  }

  testCase.mock.on('session/set_mode', () => {
    return getRegistryOverride(overrides, 'set_mode', 'session/set_mode') ?? {};
  });
  testCase.mock.on('session/cancel', () => {
    return getRegistryOverride(overrides, 'cancel', 'session/cancel') ?? {};
  });
  testCase.mock.on('session/list', () => {
    return (
      getRegistryOverride(overrides, 'list', 'session/list') ?? {
        sessions: [],
      }
    );
  });
  testCase.mock.on('session/fork', () => {
    return (
      getRegistryOverride(overrides, 'fork', 'session/fork') ?? {
        sessionId: `${sessionId}-fork`,
      }
    );
  });
}

function buildModeConfigOption() {
  return modeConfigOption('vibe', [
    { value: 'vibe', name: 'Default' },
    { value: 'plan', name: 'Plan' },
    { value: 'kiro_guide', name: 'Guide' },
  ]);
}

function buildModelOptionList() {
  return [
    { value: 'claude-sonnet-4', name: 'Claude Sonnet 4' },
    { value: 'claude-opus-4.7', name: 'Claude Opus 4.7' },
  ];
}

export function buildModelConfigOptions(currentValue: string): unknown[] {
  return [
    buildModeConfigOption(),
    modelConfigOption(currentValue, buildModelOptionList()),
    effortConfigOption('high', LEVELS),
  ];
}

export function buildEffortConfigOptions(currentValue: string): unknown[] {
  return [
    buildModeConfigOption(),
    modelConfigOption('claude-sonnet-4', buildModelOptionList()),
    effortConfigOption(currentValue, LEVELS),
  ];
}
