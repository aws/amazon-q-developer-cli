import React, { useState } from 'react';
import { Kiro } from '../../kiro.js';
import {
  AppStoreContext,
  createAppStore,
  type AppState,
  type AppStoreApi,
} from '../../stores/app-store.js';
import {
  AgentEventType,
  ApprovalOptionId,
  ContentType,
  type ApprovalRequestInfo,
} from '../../types/agent-events.js';
import type { ListSessionsResponse } from '../../types/session-client.js';
import type { SessionListingInput } from '../../utils/session-dashboard.js';
import { certifyVisualStory } from '../../storybook/story-certification.js';
import type {
  StorybookParameters,
  StorybookPlay,
} from '../../storybook/contracts.js';
import { ActivityTray } from '../ui/activity-tray/ActivityTray.js';
import { AppContainer } from '../layout/AppContainer.js';
import { InlineLayout } from '../layout/InlineLayout.js';
import { MonitorApprovalBanner } from '../layout/MonitorApprovalBanner.js';
import { TuiApprovalSurface } from '../layout/approval-surface.js';
import { TuiStatusSurface } from '../layout/tui-status-surface.js';

const viewport = { columns: 130, rows: 42 };

const meta = {
  title: 'Compositions/ApplicationShell',
  parameters: {
    layout: 'fullscreen',
    experience: 'tui',
    visualStates: {
      'inline-transcript': {
        label: 'Inline application shell with completed transcript and prompt',
      },
      'inline-status': {
        label: 'Inline status surface with model and workspace context',
      },
      'app-container-inline-routing': {
        label: 'Application host routing into the TUI inline layout',
      },
      'app-container-approval-routing': {
        label: 'Application host routing a pending TUI approval',
      },
      'app-container-expanded-placeholder': {
        label: 'Application host routing into the current expanded placeholder',
      },
      'app-container-session-dashboard': {
        label:
          'Application host routing into the full-screen session dashboard',
      },
      'session-dashboard-host-preview': {
        label: 'Full-screen session dashboard conversation preview',
      },
      'session-dashboard-host-turns': {
        label: 'Full-screen session dashboard turn-tree preview',
      },
      'inline-approval': {
        label: 'Inline application shell with a pending tool approval',
      },
      'inline-approval-allowed': {
        label: 'Inline application shell restored after allowing the tool',
      },
      'monitor-approval-banner': {
        label: 'Pending question banner while a monitor owns the screen',
      },
      'app-container-mode-routing': {
        label: 'App container routes every full-screen mode',
        gapType: 'integration-only',
        description:
          'Alternate-screen mode routing requires real terminal transitions and mode stores.',
      },
      'render-metrics-chip': {
        label: 'Development render metrics appear in the status surface',
        gapType: 'integration-only',
        description:
          'Render metrics require a renderer-owned metrics stream enabled through KIRO_DEV.',
      },
    },
    storyOrder: [
      'AppContainerInline',
      'AppContainerApproval',
      'AppContainerExpanded',
      'AppContainerSessionDashboard',
      'InlineTranscript',
      'InlineApproval',
      'MonitorQuestionBanner',
      'StatusSurface',
    ],
  },
};

export default meta;

function createInlineStore(
  withApproval = false,
  options: { kiro?: Kiro; agentEngine?: 'v2' | 'kas' } = {}
): AppStoreApi {
  const store = createAppStore({
    kiro: options.kiro ?? new Kiro(),
    agentEngine: options.agentEngine ?? 'v2',
    uiMode: 'tui',
  });
  store.setState({
    currentAgent: { name: 'kiro' },
    currentModel: { id: 'claude-sonnet', name: 'Claude Sonnet' },
    contextUsagePercent: 37,
    isInitialized: true,
    mode: 'inline',
    sessionId: 'visual-application-shell',
  });
  const emit = store.getState().createStreamEventHandler({ fromHistory: true });
  emit({
    type: AgentEventType.UserMessage,
    id: 'shell-user',
    content: {
      type: ContentType.Text,
      text: 'SHELL_PROMPT verify the release candidate.',
    },
  });
  emit({ type: AgentEventType.TurnStart });
  emit({
    type: AgentEventType.Content,
    id: 'shell-response',
    content: {
      type: ContentType.Text,
      text: 'SHELL_RESPONSE all required certification lanes passed.',
    },
  });
  emit({ type: AgentEventType.TurnEnd });
  emit.flush();

  if (withApproval) {
    const approval: ApprovalRequestInfo = {
      toolId: 'execute_bash',
      toolCall: {
        toolCallId: 'shell-approval',
        title: 'execute_bash',
        name: 'execute_bash',
        kind: 'execute',
        origin: 'builtin',
        rawInput: {
          command: 'bun run test:storybook:coverage',
          cwd: '/workspace',
        },
      },
      permissionOptions: [
        {
          kind: ApprovalOptionId.AllowOnce,
          name: 'Allow once',
          optionId: ApprovalOptionId.AllowOnce,
        },
        {
          kind: ApprovalOptionId.RejectOnce,
          name: 'Reject once',
          optionId: ApprovalOptionId.RejectOnce,
        },
      ],
      resolve: () => undefined,
    };
    const live = store.getState().createStreamEventHandler();
    live({
      type: AgentEventType.ToolCall,
      id: 'shell-approval',
      name: 'execute_bash',
      kind: 'execute',
      origin: 'builtin',
      args: {
        command: 'bun run test:storybook:coverage',
        cwd: '/workspace',
      },
    });
    live({ type: AgentEventType.ApprovalRequest, value: approval });
  }
  return store;
}

const dashboardSessions: SessionListingInput[] = [
  {
    sessionId: 'host-release-session',
    cwd: '/workspace/kiro-cli',
    title: 'Host release certification',
    updatedAt: '2026-08-19T12:00:00.000Z',
    messageCount: 14,
    engine: 'v3',
    source: 'remote',
    executionTarget: { kind: 'cloud-sandbox' },
    status: 'in_progress',
  },
  {
    sessionId: 'host-windows-session',
    cwd: '/workspace/kiro-cli',
    title: 'Host Windows verification',
    updatedAt: '2026-08-19T11:55:00.000Z',
    messageCount: 9,
    engine: 'v3',
    source: 'remote',
    executionTarget: { kind: 'cloud-sandbox' },
    status: 'idle',
  },
  {
    sessionId: 'host-cloud-session',
    cwd: '/workspace/cloud-release',
    title: 'Host cloud smoke tests',
    updatedAt: '2026-08-19T11:50:00.000Z',
    messageCount: 6,
    engine: 'v3',
    source: 'remote',
    executionTarget: { kind: 'cloud-sandbox' },
    status: 'in_progress',
  },
];

class DashboardStoryKiro extends Kiro {
  override async listSessions(_cwd: string): Promise<ListSessionsResponse> {
    return { sessions: dashboardSessions, complete: true };
  }

  override async listAllWorkspaceSessions(): Promise<ListSessionsResponse> {
    return { sessions: dashboardSessions, complete: true };
  }
}

type HostMode = 'inline' | 'expanded' | 'session-dashboard';

function createHostStore(mode: HostMode, withApproval = false): AppStoreApi {
  const dashboard = mode === 'session-dashboard';
  const store = createInlineStore(withApproval, {
    kiro: dashboard ? new DashboardStoryKiro() : new Kiro(),
    agentEngine: dashboard ? 'kas' : 'v2',
  });
  if (dashboard) {
    store.getState().setShowSessionDashboard(true, dashboardSessions, 'slash');
    store.setState({
      dashboardHighlightedSession: {
        sessionId: dashboardSessions[0]!.sessionId,
        engine: 'v3',
        source: 'remote',
      },
      sessionId: 'host-current-session',
    });
  }
  store.setState({ mode });
  return store;
}

function AppContainerSurface({
  mode,
  withApproval = false,
}: {
  mode: HostMode;
  withApproval?: boolean;
}): React.ReactElement {
  const [store] = useState(() => createHostStore(mode, withApproval));
  return (
    <AppStoreContext.Provider value={store}>
      <AppContainer />
    </AppStoreContext.Provider>
  );
}

export const AppContainerInline = {
  render: () => <AppContainerSurface mode="inline" />,
  parameters: certifyVisualStory(
    'SHELL_RESPONSE',
    {
      visible: [
        'SHELL_PROMPT verify the release candidate.',
        'SHELL_RESPONSE all required certification lanes passed.',
        'ask a question or describe a task',
        'Claude Sonnet',
      ],
      ordered: [
        'SHELL_PROMPT verify the release candidate.',
        'SHELL_RESPONSE all required certification lanes passed.',
        'ask a question or describe a task',
      ],
    },
    ['app-container-inline-routing'],
    viewport
  ),
};

export const AppContainerApproval = {
  render: () => <AppContainerSurface mode="inline" withApproval />,
  parameters: certifyVisualStory(
    'execute_bash requires approval',
    {
      visible: [
        'execute_bash requires approval',
        'bun run test:storybook:coverage',
        'Yes, single permission',
        'No (Tab to edit)',
      ],
      hidden: ['ask a question or describe a task'],
    },
    ['app-container-approval-routing'],
    viewport
  ),
};

export const AppContainerExpanded = {
  render: () => <AppContainerSurface mode="expanded" />,
  parameters: certifyVisualStory(
    'Kiro CLI Chat - Expanded Mode',
    {
      visible: [
        'Kiro CLI Chat - Expanded Mode',
        'Expanded mode - TODO: Implement full-screen chat interface',
        'Messages:',
        'Current message: No',
        'Press Escape to return to inline mode',
      ],
    },
    ['app-container-expanded-placeholder'],
    viewport
  ),
};

const captureSessionDashboardHost: StorybookPlay = async ({
  press,
  waitFor,
  capture,
}) => {
  await waitFor('Host release certification');
  await capture('list');
  await press('ctrl+p');
  await waitFor('Preview (conversation');
  await capture('conversation-preview');
  await press('shift+tab');
  await waitFor('Preview (turns');
  await capture('turns-preview');
};

const sessionDashboardParameters: StorybookParameters = {
  layout: 'fullscreen',
  experience: 'tui',
  capturesKeyboard: true,
  certification: {
    suite: 'visual-stories',
    readyText: 'Host release certification',
    viewport: { columns: 150, rows: 44 },
    environment: {
      KIRO_TEST_SESSIONS_DIR: '/tmp/kiro-visual-empty-sessions',
    },
    assertions: {
      visible: ['Sessions', 'Host release certification'],
      hidden: ['undefined'],
    },
    captures: {
      list: {
        label: 'application-hosted session dashboard list',
        coversVisualStates: ['app-container-session-dashboard'],
        assertions: {
          visible: [
            'Host release certification',
            'Host Windows verification',
            'Host cloud smoke tests',
          ],
          hidden: ['Preview (conversation', 'Preview (turns'],
        },
      },
      'conversation-preview': {
        label: 'application-hosted conversation preview',
        coversVisualStates: ['session-dashboard-host-preview'],
        assertions: {
          visible: ['Preview (conversation', 'Host release certification'],
          hidden: ['Preview (turns'],
        },
      },
      'turns-preview': {
        label: 'application-hosted turn-tree preview',
        coversVisualStates: ['session-dashboard-host-turns'],
        assertions: {
          visible: ['Preview (turns', 'Host release certification'],
          hidden: ['Preview (conversation'],
        },
      },
    },
  },
};

export const AppContainerSessionDashboard = {
  render: () => <AppContainerSurface mode="session-dashboard" />,
  parameters: sessionDashboardParameters,
  play: captureSessionDashboardHost,
};

function InlineSurface({
  withApproval = false,
}: {
  withApproval?: boolean;
}): React.ReactElement {
  const [store] = useState(() => createInlineStore(withApproval));
  return (
    <AppStoreContext.Provider value={store}>
      <InlineLayout
        ApprovalPrompt={TuiApprovalSurface}
        StatusLine={TuiStatusSurface}
        ActivityTray={ActivityTray}
      />
    </AppStoreContext.Provider>
  );
}

export const InlineTranscript = {
  render: () => <InlineSurface />,
  parameters: certifyVisualStory(
    'SHELL_RESPONSE',
    {
      visible: [
        'SHELL_PROMPT verify the release candidate.',
        'SHELL_RESPONSE all required certification lanes passed.',
        'ask a question or describe a task',
        '/copy to clipboard',
      ],
      ordered: [
        'SHELL_PROMPT verify the release candidate.',
        'SHELL_RESPONSE all required certification lanes passed.',
        'ask a question or describe a task',
      ],
      occurrences: {
        'SHELL_PROMPT verify the release candidate.': 1,
        'SHELL_RESPONSE all required certification lanes passed.': 1,
      },
    },
    ['inline-transcript', 'inline-status'],
    viewport
  ),
};

export const InlineApproval = {
  render: () => <InlineSurface withApproval />,
  parameters: {
    layout: 'fullscreen',
    experience: 'tui',
    capturesKeyboard: true,
    certification: {
      suite: 'visual-stories',
      readyText: 'execute_bash requires approval',
      viewport,
      assertions: {
        visible: ['SHELL_PROMPT verify the release candidate.'],
        hidden: ['undefined'],
      },
      captures: {
        pending: {
          label: 'inline shell approval pending',
          coversVisualStates: ['inline-approval'],
          assertions: {
            visible: [
              'SHELL_PROMPT verify the release candidate.',
              'execute_bash requires approval',
              'bun run test:storybook:coverage',
              'Yes, single permission',
              'No (Tab to edit)',
            ],
            hidden: ['ask a question or describe a task', '/copy to clipboard'],
            ordered: [
              'SHELL_PROMPT verify the release candidate.',
              'bun run test:storybook:coverage',
              'execute_bash requires approval',
            ],
          },
        },
        allowed: {
          label: 'inline shell approval allowed',
          coversVisualStates: ['inline-approval-allowed'],
          assertions: {
            visible: [
              'SHELL_PROMPT verify the release candidate.',
              'Shell bun run test:storybook:coverage',
              'ask a question or describe a task',
            ],
            hidden: ['requires approval', 'No (Tab to edit)'],
            ordered: [
              'SHELL_PROMPT verify the release candidate.',
              'Shell bun run test:storybook:coverage',
              'ask a question or describe a task',
            ],
          },
        },
      },
    },
  } satisfies StorybookParameters,
  play: (async ({ press, waitFor, capture }) => {
    await capture('pending');
    await press('enter');
    await waitFor('ask a question or describe a task');
    await capture('allowed');
  }) satisfies StorybookPlay,
};

function MonitorBannerStory(): React.ReactElement {
  const [store] = useState(() => {
    const storyStore = createInlineStore();
    const pendingQuestion: NonNullable<AppState['pendingQuestion']> = {
      sessionId: 'visual-application-shell',
      toolCallId: 'monitor-question',
      question: 'Which lane should be retried?',
      options: [{ title: 'Windows integration' }],
      resolve: () => undefined,
    };
    storyStore.setState({
      mode: 'crew-monitor',
      pendingQuestion,
      questionQueue: [pendingQuestion],
    });
    return storyStore;
  });
  return (
    <AppStoreContext.Provider value={store}>
      <MonitorApprovalBanner />
    </AppStoreContext.Provider>
  );
}

export const MonitorQuestionBanner = {
  render: MonitorBannerStory,
  parameters: certifyVisualStory(
    'The agent is asking a question',
    {
      visible: [
        'The agent is asking a question',
        'press q or Esc to return to chat and respond',
      ],
    },
    ['monitor-approval-banner'],
    { columns: 100, rows: 8 }
  ),
};

export const StatusSurface = {
  render: () => (
    <TuiStatusSurface
      agentName="kiro"
      modelName="Claude Sonnet"
      effort="high"
      contextUsagePercent={37}
      workspacePath="/workspace/kiro-cli"
      gitBranch="visual-stories"
      goalStatus={null}
      tangentName={null}
      cloudSessionActive={false}
      codeIntelligenceActive
      now={new Date('2026-08-19T12:00:00.000Z')}
      usagePercent={24}
      creditsRemaining={760}
    />
  ),
  parameters: certifyVisualStory(
    'Claude Sonnet',
    {
      visible: ['kiro', 'Claude Sonnet', '37%', 'visual-stories', 'kiro-cli'],
      ordered: ['kiro', 'Claude Sonnet'],
    },
    ['inline-status'],
    { columns: 120, rows: 6 }
  ),
};
