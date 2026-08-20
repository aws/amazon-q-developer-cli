import React, { useEffect, useState } from 'react';
import { Box, Text, useInput } from '../../renderer.js';
import { Kiro } from '../../kiro.js';
import {
  AppStoreContext,
  createAppStore,
  useAppStore,
  type AppStoreApi,
  type StreamEventHandler,
} from '../../stores/app-store.js';
import { sessionConversationsStore } from '../../stores/session-conversations.js';
import { createWorkflowStore } from '../../stores/workflow-store.js';
import {
  AgentEventType,
  ApprovalOptionId,
  ContentType,
  type AgentStreamEvent,
  type ApprovalRequestInfo,
  type PermissionResponse,
} from '../../types/agent-events.js';
import { InterruptMode } from '../../constants/interrupt-mode.js';
import { Settings } from '../../constants/settings.js';
import type {
  StorybookAssertions,
  StorybookParameters,
  StorybookPlay,
} from '../../storybook/contracts.js';
import type { WorkflowRunView } from '../../types/workflow-monitor.js';
import { VerbosityOverrideContext } from '../../hooks/useVerbose.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { DEFAULT_DISPLAY } from '../../lite/verbose.js';
import { LiteLayout } from '../layout/lite/LiteLayout.js';
import { LiteLiveRegion } from '../layout/lite/LiteLiveRegion.js';
import { LiteActivityTray } from '../layout/lite/LiteActivityTray.js';
import { LiteSubagentPanel } from '../layout/lite/LiteSubagentPanel.js';
import { ApprovalPrompt } from '../layout/lite/ApprovalPrompt.js';
import { LiteApprovalSurface } from '../layout/approval-surface.js';
import { LiteStatusSurface } from '../layout/lite/status-surface.js';

const viewport = { columns: 120, rows: 42 };
let clearToken = 40_000;
type EventSink = (event: AgentStreamEvent) => void;

const meta = {
  title: 'Compositions/LiteExperience',
  component: LiteLayout,
  parameters: {
    layout: 'fullscreen',
    experience: 'lite',
    storyOrder: [
      'TranscriptThinkingToolQueue',
      'LiveRegionLifecycle',
      'ActivityTrayRows',
      'SubagentTraceNavigation',
      'ApprovalPromptTrust',
      'ApprovalSurfaceAllow',
      'StatusSurfaceOverview',
    ],
    visualStates: {
      'layout-transcript': {
        label: 'Lite layout with completed history and an active turn',
      },
      'layout-thinking': {
        label: 'Active Lite turn showing reasoning',
      },
      'layout-tool-running': {
        label: 'Active shell tool rendered through the Lite live region',
      },
      'layout-tool-output': {
        label: 'Streaming shell output retained with the active tool',
      },
      'layout-queue': {
        label: 'Queued messages and tasks visible around the active turn',
      },
      'live-thinking': {
        label: 'Standalone Lite live region reasoning state',
      },
      'live-tool': {
        label: 'Standalone Lite live region running tool state',
      },
      'live-tool-output': {
        label: 'Standalone Lite live output stream',
      },
      'live-response': {
        label: 'Standalone Lite response after completed output is promoted',
      },
      'activity-collapsed': {
        label: 'Collapsed workflow, task, and queue summary',
      },
      'activity-expanded': {
        label: 'Expanded Lite workflow and task rows',
      },
      'subagent-following': {
        label: 'Subagent trace following its active tail',
      },
      'subagent-scrolled': {
        label: 'Subagent trace manually scrolled to earlier evidence',
      },
      'approval-pending': {
        label: 'Direct Lite approval prompt awaiting a decision',
      },
      'approval-trust-scope': {
        label: 'Direct Lite approval prompt trust-scope picker',
      },
      'approval-trusted': {
        label: 'Direct Lite approval resolved through the app store',
      },
      'approval-surface-pending': {
        label: 'Lite approval adapter awaiting a decision',
      },
      'approval-surface-allowed': {
        label: 'Lite approval adapter resolved through the app store',
      },
      'status-overview': {
        label: 'Lite agent, model, context, workspace, and branch status',
      },
      'pty-scrollback-resize': {
        label: 'Append-only scrollback retained across a real terminal resize',
        gapType: 'integration-only',
        description:
          'Requires a real PTY resize while the append-only terminal history remains active.',
      },
      'subagent-backend-kill': {
        label: 'Subagent termination confirmed by a live backend session',
        gapType: 'integration-only',
        description:
          'Requires a live backend session to acknowledge and report process termination.',
      },
      'approval-note-delivery': {
        label: 'Approval note delivered before the backend disposition',
        gapType: 'integration-only',
        description:
          'Requires the backend approval exchange to verify note and disposition ordering.',
      },
    },
  },
};

export default meta;

function certification(
  readyText: string,
  assertions: StorybookAssertions,
  coversVisualStates: readonly string[],
  captures?: NonNullable<StorybookParameters['certification']>['captures']
): StorybookParameters {
  return {
    layout: 'fullscreen',
    experience: 'lite',
    capturesKeyboard: captures !== undefined,
    ...(captures ? {} : { coversVisualStates }),
    certification: {
      suite: 'visual-stories',
      readyText,
      viewport,
      environment: { KIRO_LITE_ROLLOUT_ENABLED: '1' },
      assertions: {
        visible: assertions.visible ?? [readyText],
        hidden: ['undefined', ...(assertions.hidden ?? [])],
        ordered: assertions.ordered,
        occurrences: assertions.occurrences,
      },
      ...(captures ? { captures } : {}),
    },
  };
}

function createLiteStore(): AppStoreApi {
  const store = createAppStore({
    kiro: new Kiro(),
    agentEngine: 'v2',
    uiMode: 'lite',
  });
  store.setState({
    uiMode: 'lite',
    mode: 'inline',
    sessionId: 'lite-story-session',
    isInitialized: true,
    currentAgent: { name: 'kiro' },
    currentModel: { id: 'claude-sonnet', name: 'Claude Sonnet' },
    currentEffort: 'high',
    contextUsagePercent: 42,
    settings: {
      [Settings.CHAT_GREETING_ENABLED]: false,
      [Settings.CHAT_SHOW_THINKING_TIPS]: false,
    },
    lite: {
      ...store.getState().lite,
      welcomeEmitted: true,
      scrollbackClearToken: clearToken++,
    },
  });
  return store;
}

function LiteStoryProviders({
  store,
  children,
}: {
  store: AppStoreApi;
  children: React.ReactNode;
}): React.ReactElement {
  return (
    <AppStoreContext.Provider value={store}>
      <VerbosityOverrideContext.Provider
        value={{
          display: {
            ...DEFAULT_DISPLAY,
            showElapsed: false,
            thinkingDisplay: 'expanded',
            showThinkingContent: true,
            persistOutput: true,
          },
          filters: ['all'],
        }}
      >
        {children}
      </VerbosityOverrideContext.Provider>
    </AppStoreContext.Provider>
  );
}

function userMessage(emit: EventSink, id: string, content: string): void {
  emit({
    type: AgentEventType.UserMessage,
    id,
    content: { type: ContentType.Text, text: content },
  });
}

function assistantContent(emit: EventSink, id: string, content: string): void {
  emit({
    type: AgentEventType.Content,
    id,
    content: { type: ContentType.Text, text: content },
  });
}

function assistantThought(emit: EventSink, id: string, content: string): void {
  emit({
    type: AgentEventType.Thought,
    id,
    content: { type: ContentType.Text, text: content },
  });
}

function startTurn(emit: EventSink): void {
  emit({ type: AgentEventType.TurnStart });
}

function endTurn(emit: StreamEventHandler): void {
  emit({ type: AgentEventType.TurnEnd });
  emit.flush();
}

function startShell(emit: EventSink, id: string, command: string): void {
  emit({
    type: AgentEventType.ToolCall,
    id,
    name: 'execute_bash',
    kind: 'execute',
    origin: 'builtin',
    args: { command, working_dir: '/workspace' },
  });
}

function finishShell(emit: EventSink, id: string, output: string): void {
  emit({
    type: AgentEventType.ToolCallFinished,
    id,
    result: { status: 'success', output },
  });
}

function createTranscriptStore(): {
  store: AppStoreApi;
  emit: StreamEventHandler;
} {
  const store = createLiteStore();
  const history = store
    .getState()
    .createStreamEventHandler({ fromHistory: true });
  userMessage(
    history,
    'lite-history-user',
    'LITE_HISTORY_PROMPT review the release workflow.'
  );
  startTurn(history);
  assistantContent(
    history,
    'lite-history-response',
    'LITE_HISTORY_RESPONSE the previous verification completed.'
  );
  endTurn(history);

  const emit = store.getState().createStreamEventHandler();
  userMessage(
    emit,
    'lite-active-user',
    'LITE_ACTIVE_PROMPT inspect the remaining visual checks.'
  );
  startTurn(emit);
  assistantThought(
    emit,
    'lite-active-thinking',
    'LITE_REASONING compare the active transcript before running the check.'
  );
  store.getState().setTasks([
    {
      id: '1',
      subject: 'Inspect the Lite transcript',
      status: 'completed',
    },
    {
      id: '2',
      subject: 'Verify the visual evidence',
      status: 'pending',
    },
  ]);
  store.getState().setActiveInterruptMode(InterruptMode.QUEUE);
  store
    .getState()
    .queueMessage('LITE_QUEUED_MESSAGE publish the verification summary.');
  return { store, emit };
}

function TranscriptStory(): React.ReactElement {
  const [{ store, emit }] = useState(createTranscriptStore);
  const [step, setStep] = useState(0);

  useInput((_, key) => {
    if (!key.tab || step >= 3) return;
    const next = step + 1;
    if (next === 1) {
      startShell(emit, 'lite-shell', 'printf LITE_TOOL_COMMAND');
    } else if (next === 2) {
      emit({
        type: AgentEventType.ToolCallUpdate,
        id: 'lite-shell',
        content: {
          type: ContentType.Text,
          text: 'LITE_TOOL_STREAM_OUTPUT',
        },
      });
    } else {
      finishShell(emit, 'lite-shell', 'LITE_TOOL_RESULT passed');
      assistantContent(
        emit,
        'lite-active-response',
        'LITE_ACTIVE_RESPONSE the visual checks passed.'
      );
    }
    setStep(next);
  });

  return (
    <LiteStoryProviders store={store}>
      <LiteLayout
        ApprovalPrompt={LiteApprovalSurface}
        StatusLine={LiteStatusSurface}
        ActivityTray={LiteActivityTray}
      />
    </LiteStoryProviders>
  );
}

const captureTranscript: StorybookPlay = async ({
  press,
  waitFor,
  capture,
}) => {
  await capture('thinking-and-queue');
  await press('tab');
  await waitFor('LITE_TOOL_COMMAND');
  await capture('tool-running');
  await press('tab');
  await waitFor('LITE_TOOL_STREAM_OUTPUT');
  await capture('tool-output');
  await press('tab');
  await waitFor('LITE_ACTIVE_RESPONSE');
  await capture('response');
};

export const TranscriptThinkingToolQueue = {
  render: TranscriptStory,
  parameters: certification(
    'LITE_ACTIVE_PROMPT',
    {
      visible: [
        'LITE_HISTORY_PROMPT',
        'LITE_HISTORY_RESPONSE',
        'LITE_ACTIVE_PROMPT',
        'LITE_REASONING',
        'LITE_QUEUED_MESSAGE',
        '1 message queued',
      ],
    },
    ['layout-transcript', 'layout-thinking', 'layout-queue'],
    {
      'thinking-and-queue': {
        label: 'completed history, active reasoning, tasks, and queue',
        coversVisualStates: [
          'layout-transcript',
          'layout-thinking',
          'layout-queue',
        ],
        assertions: {
          visible: [
            'LITE_HISTORY_PROMPT',
            'LITE_HISTORY_RESPONSE',
            'LITE_ACTIVE_PROMPT',
            'LITE_REASONING',
            'LITE_QUEUED_MESSAGE',
          ],
          hidden: ['LITE_TOOL_COMMAND'],
          ordered: [
            'LITE_HISTORY_PROMPT',
            'LITE_HISTORY_RESPONSE',
            'LITE_ACTIVE_PROMPT',
            'LITE_REASONING',
            'LITE_QUEUED_MESSAGE',
          ],
        },
      },
      'tool-running': {
        label: 'shell tool active while queued work remains visible',
        coversVisualStates: ['layout-tool-running', 'layout-queue'],
        assertions: {
          visible: [
            'LITE_ACTIVE_PROMPT',
            'LITE_TOOL_COMMAND',
            'LITE_QUEUED_MESSAGE',
          ],
          hidden: ['LITE_TOOL_RESULT', 'LITE_ACTIVE_RESPONSE'],
          occurrences: {
            LITE_TOOL_COMMAND: 1,
            LITE_QUEUED_MESSAGE: 1,
          },
        },
      },
      'tool-output': {
        label: 'shell live output attached to the active tool',
        coversVisualStates: ['layout-tool-running', 'layout-tool-output'],
        assertions: {
          visible: ['LITE_TOOL_COMMAND', 'LITE_TOOL_STREAM_OUTPUT'],
          hidden: ['LITE_TOOL_RESULT', 'LITE_ACTIVE_RESPONSE'],
          ordered: ['LITE_TOOL_COMMAND', 'LITE_TOOL_STREAM_OUTPUT'],
        },
      },
      response: {
        label: 'tool result followed by the streaming assistant response',
        coversVisualStates: [
          'layout-transcript',
          'layout-tool-output',
          'layout-queue',
        ],
        assertions: {
          visible: [
            'LITE_TOOL_COMMAND',
            'LITE_TOOL_RESULT',
            'LITE_ACTIVE_RESPONSE',
            'LITE_QUEUED_MESSAGE',
          ],
          ordered: [
            'LITE_TOOL_COMMAND',
            'LITE_TOOL_RESULT',
            'LITE_ACTIVE_RESPONSE',
          ],
          occurrences: {
            LITE_TOOL_COMMAND: 1,
            LITE_TOOL_RESULT: 1,
            LITE_ACTIVE_RESPONSE: 1,
            LITE_QUEUED_MESSAGE: 1,
          },
        },
      },
    }
  ),
  play: captureTranscript,
};

function createLiveRegionStore(): {
  store: AppStoreApi;
  emit: StreamEventHandler;
} {
  const store = createLiteStore();
  const emit = store.getState().createStreamEventHandler();
  userMessage(
    emit,
    'live-user',
    'LIVE_REGION_PROMPT inspect the streaming lifecycle.'
  );
  startTurn(emit);
  assistantThought(
    emit,
    'live-thought',
    'LIVE_REGION_REASONING inspect the pending tool.'
  );
  return { store, emit };
}

function LiveRegionStory(): React.ReactElement {
  const [{ store, emit }] = useState(createLiveRegionStore);
  const [step, setStep] = useState(0);

  useInput((_, key) => {
    if (!key.tab || step >= 3) return;
    const next = step + 1;
    if (next === 1) {
      startShell(emit, 'live-shell', 'printf LIVE_REGION_TOOL');
    } else if (next === 2) {
      emit({
        type: AgentEventType.ToolCallUpdate,
        id: 'live-shell',
        content: {
          type: ContentType.Text,
          text: 'LIVE_REGION_OUTPUT',
        },
      });
    } else {
      finishShell(emit, 'live-shell', 'LIVE_REGION_RESULT passed');
      assistantContent(
        emit,
        'live-response',
        'LIVE_REGION_RESPONSE verification complete.'
      );
    }
    setStep(next);
  });

  return (
    <LiteStoryProviders store={store}>
      <Box flexDirection="column">
        <Text>Lite live region contract</Text>
        <LiteLiveRegion />
      </Box>
    </LiteStoryProviders>
  );
}

const captureLiveRegion: StorybookPlay = async ({
  press,
  waitFor,
  capture,
}) => {
  await capture('thinking');
  await press('tab');
  await waitFor('LIVE_REGION_TOOL');
  await capture('tool');
  await press('tab');
  await waitFor('LIVE_REGION_OUTPUT');
  await capture('output');
  await press('tab');
  await waitFor('LIVE_REGION_RESPONSE');
  await capture('response');
};

export const LiveRegionLifecycle = {
  render: LiveRegionStory,
  parameters: certification(
    'Lite live region contract',
    { visible: ['Lite live region contract'] },
    ['live-thinking'],
    {
      thinking: {
        label: 'reasoning before the first tool call',
        coversVisualStates: ['live-thinking'],
        assertions: {
          visible: ['LIVE_REGION_REASONING'],
          hidden: ['LIVE_REGION_TOOL'],
        },
      },
      tool: {
        label: 'running shell tool replaces idle reasoning',
        coversVisualStates: ['live-tool'],
        assertions: {
          visible: ['LIVE_REGION_TOOL'],
          hidden: [
            'LIVE_REGION_REASONING',
            'LIVE_REGION_OUTPUT',
            'LIVE_REGION_RESPONSE',
          ],
        },
      },
      output: {
        label: 'tool output streams beneath its active row',
        coversVisualStates: ['live-tool', 'live-tool-output'],
        assertions: {
          visible: ['LIVE_REGION_TOOL', 'LIVE_REGION_OUTPUT'],
          hidden: ['LIVE_REGION_REASONING', 'LIVE_REGION_RESPONSE'],
          ordered: ['LIVE_REGION_TOOL', 'LIVE_REGION_OUTPUT'],
        },
      },
      response: {
        label: 'active response replaces output promoted to scrollback',
        coversVisualStates: ['live-response'],
        assertions: {
          visible: ['LIVE_REGION_RESPONSE'],
          hidden: [
            'LIVE_REGION_REASONING',
            'LIVE_REGION_TOOL',
            'LIVE_REGION_RESULT',
            'LIVE_REGION_OUTPUT',
          ],
        },
      },
    }
  ),
  play: captureLiveRegion,
};

function runningWorkflow(): WorkflowRunView {
  return {
    workflowId: 'lite-release-workflow',
    parentSessionId: 'lite-story-session',
    name: 'release-hardening',
    status: 'running',
    nodes: [
      {
        id: 'inspect',
        type: 'step',
        status: 'completed',
        label: 'Inspect',
        parentId: null,
        depth: 0,
      },
      {
        id: 'certify',
        type: 'step',
        status: 'running',
        label: 'Certify',
        parentId: null,
        depth: 0,
      },
    ],
    stepSessions: [],
    startedAt: Date.parse('2026-08-19T17:00:00.000Z'),
    completedAt: null,
  };
}

function createActivityStores(): {
  appStore: AppStoreApi;
  workflowStore: ReturnType<typeof createWorkflowStore>;
} {
  const appStore = createLiteStore();
  appStore.getState().setTasks([
    { id: '1', subject: 'Compile the release candidate', status: 'completed' },
    { id: '2', subject: 'Review the visual evidence', status: 'pending' },
    { id: '3', subject: 'Publish the certification', status: 'pending' },
  ]);
  appStore.getState().queueMessage('ACTIVITY_QUEUE verify Windows packaging.');
  const workflowStore = createWorkflowStore();
  workflowStore.getState().openHistoricalWorkflow(runningWorkflow());
  return { appStore, workflowStore };
}

function ActivityStory(): React.ReactElement {
  const [{ appStore, workflowStore }] = useState(createActivityStores);

  useInput((_, key) => {
    if (key.tab) appStore.getState().toggleActivityTray();
  });

  return (
    <LiteStoryProviders store={appStore}>
      <LiteActivityTray store={workflowStore} />
    </LiteStoryProviders>
  );
}

const captureActivity: StorybookPlay = async ({ press, waitFor, capture }) => {
  await capture('collapsed');
  await press('tab');
  await waitFor('workflows (1)');
  await capture('expanded');
};

export const ActivityTrayRows = {
  render: ActivityStory,
  parameters: certification(
    '1 workflow running',
    { visible: [] },
    ['activity-collapsed'],
    {
      collapsed: {
        label: 'collapsed workflow, task, and queue summary',
        coversVisualStates: ['activity-collapsed'],
        assertions: {
          visible: [
            '1 workflow running',
            'steps 1/2',
            '1 message queued',
            '2 tasks remaining',
            'ctrl+x expand',
          ],
          hidden: ['workflows (1)', 'tasks (3)'],
        },
      },
      expanded: {
        label: 'expanded workflow and task rows',
        coversVisualStates: ['activity-expanded'],
        assertions: {
          visible: [
            'activity',
            'workflows (1)',
            'release-hardening',
            'running',
            'steps 1/2',
            'tasks (3)',
            'Compile the release candidate',
            '[done]',
            'Review the visual evidence',
            'Publish the certification',
            'ctrl+g monitor',
          ],
          ordered: [
            'activity',
            'workflows (1)',
            'release-hardening',
            'tasks (3)',
            'Compile the release candidate',
            'Review the visual evidence',
            'Publish the certification',
          ],
        },
      },
    }
  ),
  play: captureActivity,
};

const subagentSessionId = 'lite-story-subagent';

function seedSubagentConversation(): void {
  const conversations = sessionConversationsStore.getState();
  conversations.clearSession(subagentSessionId);
  const emit = conversations.createHandlerForSession(subagentSessionId);
  assistantContent(
    emit,
    'subagent-head',
    'SUBAGENT_HEAD inspect the cache contract.'
  );
  startShell(emit, 'subagent-read', 'printf SUBAGENT_READ_COMMAND');
  finishShell(emit, 'subagent-read', 'SUBAGENT_READ_RESULT passed');
  assistantContent(
    emit,
    'subagent-middle',
    'SUBAGENT_MIDDLE compare the generated manifest.'
  );
  startShell(emit, 'subagent-shell', 'printf SUBAGENT_SHELL_COMMAND');
  finishShell(emit, 'subagent-shell', 'SUBAGENT_SHELL_RESULT passed');
  assistantContent(
    emit,
    'subagent-tail',
    'SUBAGENT_TAIL certification evidence is ready.'
  );
}

function SubagentStory(): React.ReactElement {
  useState(() => {
    seedSubagentConversation();
    return true;
  });
  const [followBottom, setFollowBottom] = useState(true);
  const [scrollOffset, setScrollOffset] = useState(0);
  const [totalLines, setTotalLines] = useState(0);

  useEffect(
    () => () =>
      sessionConversationsStore.getState().clearSession(subagentSessionId),
    []
  );

  useInput((_, key) => {
    if (key.upArrow) {
      setFollowBottom(false);
      setScrollOffset(0);
    } else if (key.downArrow) {
      setFollowBottom(true);
      setScrollOffset(Math.max(0, totalLines - 7));
    }
  });

  return (
    <Box flexDirection="column">
      <Text>Lite subagent trace contract</Text>
      <LiteSubagentPanel
        sessionId={subagentSessionId}
        name="cache-validator"
        position={1}
        total={2}
        visibleLines={7}
        scrollOffset={scrollOffset}
        followBottom={followBottom}
        onLinesChange={setTotalLines}
        phaseLabel="running"
        canKill={false}
      />
    </Box>
  );
}

const captureSubagent: StorybookPlay = async ({ press, waitFor, capture }) => {
  await waitFor('SUBAGENT_TAIL');
  await capture('following');
  await press('up');
  await waitFor('SUBAGENT_HEAD');
  await capture('scrolled');
  await press('down');
  await waitFor('SUBAGENT_TAIL');
  await capture('following-restored');
};

export const SubagentTraceNavigation = {
  render: SubagentStory,
  parameters: certification(
    'Lite subagent trace contract',
    {
      visible: ['Lite subagent trace contract', '[cache-validator]', 'running'],
    },
    ['subagent-following'],
    {
      following: {
        label: 'active subagent trace pinned to the bottom',
        coversVisualStates: ['subagent-following'],
        assertions: {
          visible: [
            '[cache-validator]',
            '1/2',
            'running',
            'live',
            'SUBAGENT_TAIL',
          ],
          hidden: ['SUBAGENT_HEAD'],
        },
      },
      scrolled: {
        label: 'manual navigation reveals earlier subagent evidence',
        coversVisualStates: ['subagent-scrolled'],
        assertions: {
          visible: [
            '[cache-validator]',
            'SUBAGENT_HEAD',
            'SUBAGENT_READ_COMMAND',
          ],
          hidden: ['SUBAGENT_TAIL', ' live'],
          ordered: [
            'SUBAGENT_HEAD',
            'SUBAGENT_READ_COMMAND',
            'SUBAGENT_READ_RESULT',
          ],
        },
      },
      'following-restored': {
        label: 'down navigation restores tail following',
        coversVisualStates: ['subagent-following'],
        assertions: {
          visible: ['live', 'SUBAGENT_TAIL'],
          hidden: ['SUBAGENT_HEAD'],
        },
      },
    }
  ),
  play: captureSubagent,
};

function approvalFixture(
  id: string,
  onResolve: (response: PermissionResponse) => void
): ApprovalRequestInfo {
  return {
    originSessionId: 'lite-story-session',
    toolId: 'execute_bash',
    toolCall: {
      toolCallId: id,
      title: 'execute_bash',
      name: 'execute_bash',
      kind: 'execute',
      origin: 'builtin',
      rawInput: {
        command: 'git status && echo LITE_APPROVAL_COMMAND',
        cwd: '/workspace',
      },
    },
    permissionOptions: [
      {
        kind: ApprovalOptionId.AllowOnce,
        name: 'Allow once',
        optionId: 'allow-once',
      },
      {
        kind: ApprovalOptionId.AllowAlways,
        name: 'Always allow',
        optionId: 'allow-always',
      },
      {
        kind: ApprovalOptionId.RejectOnce,
        name: 'Reject once',
        optionId: 'reject-once',
      },
    ],
    trustOptions: [
      {
        label: 'Trust git status',
        display: 'git status',
        setting_key: 'shell',
        patterns: ['git status'],
      },
    ],
    resolve: onResolve,
  };
}

function createApprovalStoryStore(
  id: string,
  onResolve: (response: PermissionResponse) => void
): AppStoreApi {
  const store = createLiteStore();
  const emit = store.getState().createStreamEventHandler();
  startShell(emit, id, 'git status && echo LITE_APPROVAL_COMMAND');
  emit({
    type: AgentEventType.ApprovalRequest,
    value: approvalFixture(id, onResolve),
  });
  return store;
}

function ApprovalResolved({
  response,
}: {
  response: PermissionResponse | null;
}): React.ReactElement {
  const approvalCount = useAppStore((state) => state.approvalQueue.length);
  const outcome =
    response?.outcome === 'selected'
      ? `selected ${response.optionId}`
      : (response?.outcome ?? 'pending');
  return (
    <Text>{`Approval contract: ${outcome} | queue=${approvalCount}`}</Text>
  );
}

function ApprovalContent({
  direct,
  response,
}: {
  direct: boolean;
  response: PermissionResponse | null;
}): React.ReactElement {
  const messages = useAppStore((state) => state.messages);
  const approval = useAppStore((state) => state.pendingApproval);
  const respondToApproval = useAppStore((state) => state.respondToApproval);
  const { getColor } = useTheme();

  return (
    <Box flexDirection="column">
      <Text>{direct ? 'Direct approval prompt' : 'Lite approval surface'}</Text>
      {approval &&
        (direct ? (
          <ApprovalPrompt
            messages={messages}
            approval={approval}
            respondToApproval={respondToApproval}
            getStageInputColor={() => getColor('brand')}
            mainAgentName="kiro"
            onNotesSubmit={() => undefined}
          />
        ) : (
          <LiteApprovalSurface
            messages={messages}
            approval={approval}
            respondToApproval={respondToApproval}
            getStageInputColor={() => getColor('brand')}
            mainAgentName="kiro"
            onInputSubmit={() => undefined}
          />
        ))}
      <ApprovalResolved response={response} />
    </Box>
  );
}

function ApprovalStory({ direct }: { direct: boolean }): React.ReactElement {
  const [response, setResponse] = useState<PermissionResponse | null>(null);
  const [store] = useState(() =>
    createApprovalStoryStore(
      direct ? 'direct-approval' : 'surface-approval',
      setResponse
    )
  );
  return (
    <LiteStoryProviders store={store}>
      <ApprovalContent direct={direct} response={response} />
    </LiteStoryProviders>
  );
}

const captureApprovalTrust: StorybookPlay = async ({
  press,
  type,
  waitFor,
  capture,
}) => {
  await capture('pending');
  await type('t');
  await waitFor('trust scope');
  await capture('trust-scope');
  await press('enter');
  await waitFor('Approval contract: selected allow-always');
  await capture('trusted');
};

export const ApprovalPromptTrust = {
  render: () => <ApprovalStory direct />,
  parameters: certification(
    'Direct approval prompt',
    { visible: ['Direct approval prompt'] },
    ['approval-pending'],
    {
      pending: {
        label: 'direct approval prompt before a decision',
        coversVisualStates: ['approval-pending'],
        assertions: {
          visible: [
            'Shell',
            'needs approval',
            'LITE_APPROVAL_COMMAND',
            '[y]',
            '[t]',
            '[n]',
            'Approval contract: pending | queue=1',
          ],
        },
      },
      'trust-scope': {
        label: 'direct approval prompt trust picker',
        coversVisualStates: ['approval-trust-scope'],
        assertions: {
          visible: [
            'Shell',
            'trust scope',
            'Trust git status',
            'Trust entire tool',
            '[enter] confirm',
            '[esc] back',
          ],
          hidden: ['needs approval', '[y]'],
        },
      },
      trusted: {
        label: 'trust decision resolved through respondToApproval',
        coversVisualStates: ['approval-trusted'],
        assertions: {
          visible: ['Approval contract: selected allow-always | queue=0'],
          hidden: ['needs approval', 'trust scope'],
        },
      },
    }
  ),
  play: captureApprovalTrust,
};

const captureApprovalAllow: StorybookPlay = async ({
  type,
  waitFor,
  capture,
}) => {
  await capture('pending');
  await type('y');
  await waitFor('Approval contract: selected allow-once');
  await capture('allowed');
};

export const ApprovalSurfaceAllow = {
  render: () => <ApprovalStory direct={false} />,
  parameters: certification(
    'Lite approval surface',
    { visible: ['Lite approval surface'] },
    ['approval-surface-pending'],
    {
      pending: {
        label: 'Lite approval adapter delegates to ApprovalPrompt',
        coversVisualStates: ['approval-surface-pending'],
        assertions: {
          visible: [
            'Lite approval surface',
            'Shell',
            'needs approval',
            'LITE_APPROVAL_COMMAND',
            'Approval contract: pending | queue=1',
          ],
        },
      },
      allowed: {
        label: 'adapter allow decision resolves through the app store',
        coversVisualStates: ['approval-surface-allowed'],
        assertions: {
          visible: ['Approval contract: selected allow-once | queue=0'],
          hidden: ['needs approval'],
        },
      },
    }
  ),
  play: captureApprovalAllow,
};

function StatusStory(): React.ReactElement {
  const [store] = useState(createLiteStore);
  return (
    <LiteStoryProviders store={store}>
      <Box flexDirection="column">
        <Text>Lite status surface contract</Text>
        <LiteStatusSurface
          agentName="kiro"
          modelName="Claude Sonnet"
          effort="high"
          contextUsagePercent={42}
          workspacePath="/workspace/kiro-cli"
          gitBranch="visual-stories"
          goalStatus={null}
          tangentName={null}
          cloudSessionActive={false}
          cloudRepo={null}
          cloudBranch={null}
          cloudExtraRepos={0}
          codeIntelligenceActive={false}
          pendingAgentName={null}
          animationFrame={0}
          now={new Date('2026-08-19T17:00:00.000Z')}
          usagePercent={null}
          creditsRemaining={null}
        />
      </Box>
    </LiteStoryProviders>
  );
}

export const StatusSurfaceOverview = {
  render: StatusStory,
  parameters: certification(
    'Lite status surface contract',
    {
      visible: [
        'Lite status surface contract',
        'kiro',
        'Claude Sonnet',
        'high',
        '42%',
        '/workspace/kiro-cli',
        'visual-stories',
      ],
      ordered: [
        'kiro',
        'Claude Sonnet',
        'high',
        '42%',
        '/workspace/kiro-cli',
        'visual-stories',
      ],
    },
    ['status-overview']
  ),
};
