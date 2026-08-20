import React, { useState } from 'react';
import { Box } from '../../renderer.js';
import { Kiro } from '../../kiro.js';
import {
  AppStoreContext,
  createAppStore,
  MessageRole,
  type AppStoreApi,
  type CodePanelData,
  type UsageData,
} from '../../stores/app-store.js';
import { sessionConversationsStore } from '../../stores/session-conversations.js';
import type {
  StorybookAssertions,
  StorybookParameters,
} from '../../storybook/contracts.js';
import type { AgentSession } from '../../types/multi-session.js';
import type { ContextBreakdownData } from '../../types/context.js';
import type { SessionListingInput } from '../../utils/session-dashboard.js';
import { SessionViewScreen } from '../layout/SessionViewScreen.js';
import { BackendPanels } from '../layout/shared/BackendPanels.js';
import { useBackendPanelHandlers } from '../layout/shared/useBackendPanelHandlers.js';
import { SessionList } from '../multi-agent/SessionList.js';
import { SessionStatusBar } from '../multi-agent/SessionStatusBar.js';
import { CodePanel } from '../ui/CodePanel.js';
import { ContextBreakdown } from '../ui/ContextBreakdown.js';
import { KnowledgePanel } from '../ui/KnowledgePanel.js';
import { MemoriesPanel } from '../ui/MemoriesPanel.js';
import { SessionDashboard } from '../ui/SessionDashboard.js';
import { StatsPanel } from '../ui/StatsPanel.js';
import { Text } from '../ui/text/Text.js';
import { ToolsPanel } from '../ui/ToolsPanel.js';
import { UsagePanel } from '../ui/UsagePanel.js';

type OperationalScenario =
  | 'session-dashboard'
  | 'session-operations'
  | 'session-view'
  | 'backend-mcp'
  | 'tools'
  | 'code'
  | 'knowledge'
  | 'memories'
  | 'usage'
  | 'stats'
  | 'context';

interface OperationalPanelsStoryProps {
  scenario: OperationalScenario;
}

const viewport = { columns: 140, rows: 40 };

const meta = {
  title: 'Compositions/OperationalPanels',
  component: OperationalPanelsStory,
  parameters: {
    layout: 'fullscreen',
    experience: 'tui',
    visualStates: {
      'session-dashboard-list': {
        label: 'Grouped session dashboard with active and cloud sessions',
      },
      'session-dashboard-screen': {
        label: 'Full-screen dashboard host with live session catalog',
        gapType: 'integration-only',
        description:
          'The host immediately refreshes from process, filesystem, and session-list sources, so an isolated seeded frame would race the production effect.',
      },
      'session-dashboard-live-refresh': {
        label: 'Dashboard catalog refresh from live and filesystem sources',
        gapType: 'integration-only',
        description:
          'The host refresh crosses process, filesystem, and session-list boundaries that the isolated story runner cannot deterministically own.',
      },
      'session-dashboard-wide-preview': {
        label: 'Full-screen dashboard conversation and turn-tree preview',
        gapType: 'integration-only',
        description:
          'The preview reads persisted transcripts and child-session metadata from the real session stores.',
      },
      'session-list-mixed-status': {
        label: 'Session list with busy, completed, failed, and idle agents',
      },
      'session-status-summary': {
        label: 'Selected session status, message count, and unread count',
      },
      'session-view-conversation': {
        label: 'Active subagent conversation with prompt ownership',
      },
      'session-view-send-failure': {
        label: 'Active session send failure notification',
        gapType: 'integration-only',
        description:
          'The failure is emitted by the live session client after an asynchronous send attempt.',
      },
      'backend-panels-mcp': {
        label: 'Shared backend panel router rendering MCP status',
      },
      'mcp-status-matrix': {
        label: 'MCP running, failed, and authentication-required states',
      },
      'mcp-action-pending': {
        label: 'MCP add, remove, and OAuth action in progress',
        gapType: 'integration-only',
        description:
          'Pending actions require a command-capable backend and OAuth lifecycle rather than a visual fixture.',
      },
      'tools-permission-matrix': {
        label: 'Tools grouped by source and permission status',
      },
      'code-workspace-health': {
        label: 'Code intelligence workspace and LSP health',
      },
      'code-live-refresh': {
        label: 'Code intelligence refresh while initialization changes',
        gapType: 'integration-only',
        description:
          'Refresh executes the backend code command and depends on an initialized workspace service.',
      },
      'knowledge-indexing': {
        label: 'Knowledge bases with ready and indexing entries',
      },
      'memories-guidance': {
        label: 'Memories explanation and account-management destination',
      },
      'usage-plan': {
        label: 'Plan usage, add-on credits, and bonus credits',
      },
      'stats-mixed-results': {
        label: 'Request statistics with success, tool use, and failure rows',
      },
      'context-expanded': {
        label: 'Expanded context allocation with files and tool groups',
      },
    },
    storyOrder: [
      'SessionDashboardCatalog',
      'SessionOperationsJourney',
      'ActiveSessionConversation',
      'BackendMcpStatus',
      'ToolsPermissionMatrix',
      'CodeWorkspaceHealth',
      'KnowledgeIndexing',
      'MemoriesGuidance',
      'UsagePlan',
      'RequestStatistics',
      'ExpandedContext',
    ],
  },
};

export default meta;

function certification(
  readyText: string,
  assertions: StorybookAssertions,
  coversVisualStates: readonly string[],
  options: {
    rows?: number;
    captures?: NonNullable<
      NonNullable<StorybookParameters['certification']>['captures']
    >;
  } = {}
): StorybookParameters {
  return {
    layout: 'fullscreen',
    experience: 'tui',
    ...(options.captures ? {} : { coversVisualStates }),
    ...(options.captures ? { capturesKeyboard: true } : {}),
    certification: {
      suite: 'visual-stories',
      readyText,
      viewport: { ...viewport, rows: options.rows ?? viewport.rows },
      assertions: {
        visible: assertions.visible ?? [readyText],
        hidden: ['undefined', ...(assertions.hidden ?? [])],
        ordered: assertions.ordered,
        occurrences: assertions.occurrences,
      },
      ...(options.captures ? { captures: options.captures } : {}),
    },
  };
}

function activeSession(
  id: string,
  name: string,
  status: AgentSession['status'],
  overrides: Partial<AgentSession> = {}
): AgentSession {
  return {
    id,
    name,
    role: 'operator',
    status,
    type: 'ephemeral',
    created: new Date(Date.now() - 125_000),
    lastActivity: new Date(),
    ...overrides,
  };
}

function dashboardSessions(): SessionListingInput[] {
  const now = Date.now();
  return [
    {
      sessionId: 'session-release-active',
      cwd: '/workspace/kiro-cli',
      title: 'Release certification investigation',
      updatedAt: new Date(now - 60_000).toISOString(),
      messageCount: 18,
      engine: 'v3',
      source: 'local',
    },
    {
      sessionId: 'session-windows-linker',
      cwd: '/workspace/kiro-cli',
      title: 'Windows linker regression',
      updatedAt: new Date(now - 8 * 60_000).toISOString(),
      messageCount: 11,
      engine: 'v2',
      source: 'local',
    },
    {
      sessionId: 'session-cloud-assets',
      cwd: '/workspace/remote-release',
      title: 'Cloud asset verification',
      updatedAt: new Date(now - 20 * 60_000).toISOString(),
      messageCount: 7,
      engine: 'v3',
      source: 'remote',
      executionTarget: { kind: 'cloud-sandbox' },
    },
  ];
}

const usageData: UsageData = {
  planName: 'Kiro Pro',
  billingCycleReset: 'Sep 1, 2026',
  overagesEnabled: true,
  isEnterprise: false,
  usageBreakdowns: [
    {
      displayName: 'Agent requests',
      used: 72.5,
      limit: 100,
      percentage: 72.5,
      currentOverages: 0,
      overageRate: 0,
      overageCharges: 0,
      currency: 'USD',
      hasLimit: true,
    },
    {
      displayName: 'Spec generations',
      used: 14,
      limit: 0,
      percentage: 0,
      currentOverages: 0,
      overageRate: 0,
      overageCharges: 0,
      currency: 'USD',
      hasLimit: false,
    },
  ],
  bonusCredits: [
    {
      name: 'Launch bonus',
      used: 8,
      total: 25,
      daysUntilExpiry: 21,
    },
  ],
  addOnCredits: [
    {
      used: 12.5,
      total: 50,
      expiresAt: 'Oct 15, 2026',
      isActive: true,
    },
  ],
  overageCapable: true,
};

const codeData: CodePanelData = {
  status: 'initialized',
  rootPath: '/workspace/kiro-cli',
  detectedLanguages: ['Rust', 'TypeScript'],
  projectMarkers: ['Cargo.toml', 'package.json'],
  lsps: [
    {
      name: 'rust-analyzer',
      languages: ['Rust'],
      status: 'initialized',
      isAvailable: true,
      initDurationMs: 842,
      workspaceFolders: ['/workspace/kiro-cli'],
    },
    {
      name: 'typescript-language-server',
      languages: ['TypeScript', 'TSX'],
      status: 'initializing',
      isAvailable: true,
      initDurationMs: 1_240,
      workspaceFolders: ['/workspace/kiro-cli/packages/tui'],
    },
  ],
  configPath: '/workspace/kiro-cli/.kiro/settings/lsp.json',
  docUrl: 'https://kiro.dev/docs/code-intelligence',
  warning: 'One language server is still indexing.',
};

const contextBreakdown: ContextBreakdownData = {
  contextFiles: {
    percent: 24.5,
    tokens: 24_500,
    items: [
      {
        name: 'AGENTS.md',
        tokens: 8_200,
        matched: true,
        percent: 8.2,
        autoIncluded: true,
      },
      {
        name: 'release-design.md',
        tokens: 4_100,
        matched: false,
        percent: 4.1,
      },
    ],
  },
  tools: {
    percent: 18,
    tokens: 18_000,
    groups: [
      {
        name: 'Built-in tools',
        source: 'builtin',
        tokens: 11_000,
        percent: 11,
        items: [
          { name: 'fs_read', tokens: 4_000, percent: 4 },
          { name: 'execute_bash', tokens: 7_000, percent: 7 },
        ],
      },
      {
        name: 'GitHub MCP',
        source: 'github',
        tokens: 7_000,
        percent: 7,
        items: [{ name: 'pull_request_read', tokens: 7_000, percent: 7 }],
      },
    ],
  },
  kiroResponses: { percent: 20, tokens: 20_000 },
  yourPrompts: { percent: 12.5, tokens: 12_500 },
  sessionFiles: {
    percent: 3,
    tokens: 3_000,
    items: [
      {
        name: 'verification.log',
        tokens: 3_000,
        matched: true,
        percent: 3,
      },
    ],
  },
  initialExpanded: true,
};

function configureStore(
  store: AppStoreApi,
  scenario: OperationalScenario
): void {
  store.setState({
    currentAgent: { name: 'release-operator' },
    currentModel: { id: 'claude-sonnet', name: 'Claude Sonnet' },
    contextUsagePercent: 75,
  });

  if (scenario === 'session-view') {
    const session = activeSession(
      'session-release-verifier',
      'release-verifier',
      'busy'
    );
    sessionConversationsStore.setState({
      conversations: new Map([
        [
          session.id,
          [
            {
              id: 'session-user',
              role: MessageRole.User,
              content: 'Verify the three release binaries before promotion.',
            },
            {
              id: 'session-model',
              role: MessageRole.Model,
              content:
                'Linux and macOS passed. Windows signature verification is still running.',
            },
          ],
        ],
      ]),
    });
    store.setState({
      sessions: new Map([[session.id, session]]),
      activeSessionId: session.id,
      selectedSessionId: session.id,
    });
  }

  if (scenario === 'backend-mcp') {
    store.setState({
      showMcpPanel: true,
      mcpMode: 'status',
      mcpServers: [
        {
          name: 'filesystem',
          status: 'running',
          toolCount: 8,
        },
        {
          name: 'github',
          status: 'failed',
          toolCount: 0,
        },
        {
          name: 'artifact-reports',
          status: 'auth-required',
          toolCount: 4,
        },
      ],
      pendingOAuthServers: new Map([
        ['artifact-reports', 'https://auth.example.com/device'],
      ]),
      initErrors: [
        {
          type: 'mcp_failure',
          serverName: 'github',
          error: 'Connection closed during initialize',
        },
      ],
    });
  }
}

function createOperationalStore(scenario: OperationalScenario): AppStoreApi {
  const store = createAppStore({
    kiro: new Kiro(),
    agentEngine: 'v2',
    uiMode: 'tui',
  });
  configureStore(store, scenario);
  return store;
}

function BackendPanelSurface(): React.ReactElement {
  const handlers = useBackendPanelHandlers();
  return <BackendPanels handlers={handlers} surface="tui" />;
}

function SessionOperationsSurface(): React.ReactElement {
  const sessions = [
    activeSession('session-busy', 'binary-builder', 'busy'),
    activeSession('session-done', 'source-auditor', 'terminated', {
      summary: 'Source audit passed.',
    }),
    activeSession('session-failed', 'windows-packager', 'failed'),
    activeSession('session-idle', 'release-writer', 'idle'),
  ];
  const selected = sessions[0]!;

  return (
    <Box flexDirection="column" width={100}>
      <Box>
        <Box flexDirection="column" width={42}>
          <Text>Operational sessions</Text>
          <SessionList
            sessions={sessions}
            selectedId={selected.id}
            onSelect={() => undefined}
            width={40}
          />
        </Box>
        <Box flexDirection="column" flexGrow={1}>
          <SessionStatusBar
            session={selected}
            messageCount={12}
            unreadCount={3}
          />
        </Box>
      </Box>
    </Box>
  );
}

function OperationalSurface({
  scenario,
}: OperationalPanelsStoryProps): React.ReactElement {
  switch (scenario) {
    case 'session-dashboard':
      return (
        <SessionDashboard
          sessions={dashboardSessions()}
          currentCwd="/workspace/kiro-cli"
          activeSessionId="session-release-active"
          activeSessionEngine="v3"
          activeSessionSource="local"
          hidePreview
          width={118}
          onSelect={() => undefined}
          onClose={() => undefined}
          backgroundReady={false}
        />
      );
    case 'session-operations':
      return <SessionOperationsSurface />;
    case 'session-view':
      return <SessionViewScreen />;
    case 'backend-mcp':
      return <BackendPanelSurface />;
    case 'tools':
      return (
        <ToolsPanel
          tools={[
            {
              name: 'fs_read',
              source: 'built-in',
              status: 'allowed',
              description: 'Read files from the workspace.',
            },
            {
              name: 'execute_bash',
              source: 'built-in',
              status: 'requires-approval',
              description: 'Run a shell command in the workspace.',
            },
            {
              name: 'publish_report',
              source: 'artifact-mcp',
              status: 'denied',
              description: 'Publish a certification report.',
            },
          ]}
          onClose={() => undefined}
        />
      );
    case 'code':
      return (
        <CodePanel
          data={codeData}
          onClose={() => undefined}
          onRefresh={() => undefined}
        />
      );
    case 'knowledge':
      return (
        <KnowledgePanel
          entries={[
            {
              name: 'release-runbook',
              id: 'kb-release',
              description: 'Release certification guidance',
              item_count: 42,
              path: '/workspace/docs/release',
              items_display: '42 items',
            },
            {
              name: 'incident-history',
              id: 'kb-incidents',
              description: 'Historical incident reports',
              item_count: 17,
              path: '/workspace/docs/incidents',
              items_display: 'indexing 9 of 17',
              indexing: true,
            },
          ]}
          status="Indexing incident-history"
          onClose={() => undefined}
        />
      );
    case 'memories':
      return <MemoriesPanel onClose={() => undefined} />;
    case 'usage':
      return <UsagePanel data={usageData} onClose={() => undefined} />;
    case 'stats':
      return (
        <StatsPanel
          stats={[
            {
              request_id: 'req-release-plan',
              timestamp: '2026-08-19T12:00:00Z',
              duration_ms: 1_280,
              ttfc_ms: 210,
              input_tokens: 8_400,
              output_tokens: 930,
              status_code: 200,
              had_tool_use: false,
              error: null,
            },
            {
              request_id: 'req-asset-check',
              timestamp: '2026-08-19T12:02:00Z',
              duration_ms: 6_240,
              ttfc_ms: 330,
              input_tokens: 11_200,
              output_tokens: 1_440,
              status_code: 200,
              had_tool_use: true,
              error: null,
            },
            {
              request_id: 'req-windows-sign',
              timestamp: '2026-08-19T12:04:00Z',
              duration_ms: 890,
              ttfc_ms: null,
              input_tokens: 4_100,
              output_tokens: 0,
              status_code: 503,
              had_tool_use: false,
              error: 'signing service unavailable',
            },
          ]}
          summary={{
            avg_ms: 2_803,
            p90_ms: 6_240,
            max_ms: 6_240,
            errors: 1,
          }}
          onClose={() => undefined}
        />
      );
    case 'context':
      return (
        <ContextBreakdown
          percent={75}
          breakdown={contextBreakdown}
          model="Claude Sonnet"
          agentName="release-operator"
          initialExpanded
          onClose={() => undefined}
        />
      );
  }
}

function OperationalPanelsStory({
  scenario,
}: OperationalPanelsStoryProps): React.ReactElement {
  const [store] = useState(() => createOperationalStore(scenario));
  return (
    <AppStoreContext.Provider value={store}>
      <OperationalSurface scenario={scenario} />
    </AppStoreContext.Provider>
  );
}

export const SessionDashboardCatalog = {
  args: { scenario: 'session-dashboard' },
  parameters: certification(
    'Release certification investigation',
    {
      visible: [
        'Sessions',
        'find by title, prompt or #tag',
        'Release certification investigation',
        'Windows linker regression',
        'Cloud asset verification',
        'of 3 sessions',
      ],
      ordered: [
        'Sessions',
        'Cloud asset verification',
        'Release certification investigation',
        'Windows linker regression',
        'of 3 sessions',
      ],
    },
    ['session-dashboard-list']
  ),
};

export const SessionOperationsJourney = {
  args: { scenario: 'session-operations' },
  parameters: certification(
    'Operational sessions',
    {
      visible: [
        'Operational sessions',
        'binary-builder',
        'source-auditor',
        'windows-packager',
        'release-writer',
        '12msg (3 unread)',
      ],
    },
    ['session-list-mixed-status', 'session-status-summary']
  ),
};

export const ActiveSessionConversation = {
  args: { scenario: 'session-view' },
  parameters: certification(
    'Verify the three release binaries before promotion.',
    {
      visible: [
        'release-verifier',
        'busy',
        'Verify the three release binaries before promotion.',
        'Linux and macOS passed.',
        'Windows signature verification is still running.',
        'message',
        'release-verifier',
      ],
      ordered: [
        'release-verifier',
        'Verify the three release binaries before promotion.',
        'Linux and macOS passed.',
        'Windows signature verification is still running.',
      ],
    },
    ['session-view-conversation']
  ),
};

export const BackendMcpStatus = {
  args: { scenario: 'backend-mcp' },
  parameters: certification(
    'filesystem',
    {
      visible: [
        '/mcp',
        'filesystem',
        'running',
        '8 tools',
        'github',
        'failed',
        'Connection closed during initialize',
        'artifact-reports',
        'auth-required',
        'Enter to authenticate',
      ],
      ordered: ['filesystem', 'github', 'artifact-reports'],
    },
    ['backend-panels-mcp', 'mcp-status-matrix']
  ),
};

export const ToolsPermissionMatrix = {
  args: { scenario: 'tools' },
  parameters: certification(
    'fs_read',
    {
      visible: [
        '/tools',
        'Name',
        'Source',
        'Status',
        'fs_read',
        'allowed',
        'execute_bash',
        'approval required',
        'publish_report',
        'denied',
      ],
      ordered: ['execute_bash', 'fs_read', 'publish_report'],
    },
    ['tools-permission-matrix']
  ),
};

export const CodeWorkspaceHealth = {
  args: { scenario: 'code' },
  parameters: certification(
    '/workspace/kiro-cli',
    {
      visible: [
        '/code',
        'initialized',
        'One language server is still indexing.',
        'Workspace:',
        '/workspace/kiro-cli',
        'Languages:',
        'Rust, TypeScript',
        'rust-analyzer',
        'typescript-language-server',
        'Config:',
      ],
      ordered: [
        'initialized',
        'Workspace:',
        'Languages:',
        'LSP Servers:',
        'rust-analyzer',
        'typescript-language-server',
        'Config:',
      ],
    },
    ['code-workspace-health']
  ),
};

export const KnowledgeIndexing = {
  args: { scenario: 'knowledge' },
  parameters: certification(
    'release-runbook',
    {
      visible: [
        '/knowledge',
        '1 entry',
        'indexing in progress',
        'release-runbook',
        'kb-release',
        '42 items',
        'incident-history',
        'kb-incidents',
        'indexing 9 of 17',
      ],
      ordered: ['release-runbook', 'incident-history'],
    },
    ['knowledge-indexing']
  ),
};

export const MemoriesGuidance = {
  args: { scenario: 'memories' },
  parameters: certification(
    'When turned on, memories will be collected',
    {
      visible: [
        '/memories',
        'memories will be collected at the end of each session',
        'learn your preferences over time',
        'https://app.kiro.dev/settings/memory',
      ],
      ordered: [
        'memories will be collected',
        'https://app.kiro.dev/settings/memory',
      ],
    },
    ['memories-guidance'],
    { rows: 18 }
  ),
};

export const UsagePlan = {
  args: { scenario: 'usage' },
  parameters: certification(
    'Estimated Usage',
    {
      visible: [
        '/usage',
        'Estimated Usage',
        'resets on Sep 1, 2026',
        'Kiro Pro',
        'Agent requests',
        '72.50 of 100 covered in plan',
        '72.5%',
        'Spec generations',
        '14.00 used',
        'Additional credits',
        '12.50 of 50 used, expires Oct 15, 2026',
        'Bonus Credits:',
        'Launch bonus: 8/25',
      ],
      ordered: [
        'Estimated Usage',
        'Agent requests',
        'Spec generations',
        'Additional credits',
        'Bonus Credits:',
      ],
    },
    ['usage-plan']
  ),
};

export const RequestStatistics = {
  args: { scenario: 'stats' },
  parameters: certification(
    'req-release-plan',
    {
      visible: [
        '/stats',
        '3 requests',
        'req-release-plan',
        '1280ms',
        'req-asset-check',
        '6240ms',
        'ok (tool_use)',
        'req-windows-sign',
        'ERR: signing service unavailable',
        'avg=2803ms',
        'errors=1',
      ],
      ordered: ['req-release-plan', 'req-asset-check', 'req-windows-sign'],
    },
    ['stats-mixed-results']
  ),
};

export const ExpandedContext = {
  args: { scenario: 'context' },
  parameters: certification(
    '75% context used',
    {
      visible: [
        '/context',
        'Current context window:',
        '75% context used',
        'Agent files',
        'Tools',
        'Kiro responses',
        'Your prompts',
        'Active agent context: release-operator',
        'AGENTS.md',
        'release-design.md',
        '(no matches)',
        'verification.log',
        'Built-in tools',
        'fs_read',
        'execute_bash',
        'GitHub MCP',
        'pull_request_read',
        '/compact',
        '/clear',
      ],
      ordered: [
        'Current context window:',
        'Agent files',
        'Active agent context: release-operator',
        'AGENTS.md',
        'Session (temporary)',
        'verification.log',
        'Built-in tools',
        'GitHub MCP',
        'Tips:',
      ],
    },
    ['context-expanded']
  ),
};
