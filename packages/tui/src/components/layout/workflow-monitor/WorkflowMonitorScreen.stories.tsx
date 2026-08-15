import React, { useState } from 'react';
import type { StoreApi } from 'zustand';
import { Kiro } from '../../../kiro.js';
import {
  AppStoreContext,
  createAppStore,
  type AppStoreApi,
} from '../../../stores/app-store.js';
import { sessionConversationsStore } from '../../../stores/session-conversations.js';
import {
  createWorkflowStore,
  type WorkflowStoreState,
} from '../../../stores/workflow-store.js';
import {
  AgentEventType,
  ApprovalOptionId,
  ContentType,
  type AgentStreamEvent,
  type ApprovalRequestInfo,
} from '../../../types/agent-events.js';
import {
  SessionLifecycleOwner,
  type AgentSession,
  type SessionStatus,
} from '../../../types/multi-session.js';
import type {
  WorkflowMonitorLayout,
  WorkflowMonitorNode,
  WorkflowRunView,
} from '../../../types/workflow-monitor.js';
import type {
  WorkflowNodeStatus,
  WorkflowNodeSessionTarget,
  WorkflowStatus,
} from '../../../types/workflow.js';
import type {
  StorybookParameters,
  StorybookPlay,
} from '../../../storybook/contracts.js';
import { WorkflowMonitorScreen } from './WorkflowMonitorScreen.js';

const STORY_NOW = Date.parse('2026-07-20T17:00:00.000Z');
const storyNow = () => STORY_NOW;

type WorkflowMonitorScenario =
  | 'running'
  | 'need-input'
  | 'completed'
  | 'failed'
  | 'multi-workflow'
  | 'stacked'
  | 'narrow'
  | 'approval'
  | 'stop-confirmation'
  | 'mouse-mode';

interface WorkflowMonitorStoryProps {
  scenario: WorkflowMonitorScenario;
}

interface WorkflowFixture {
  workflows: WorkflowRunView[];
  activeWorkflowId: string;
  selectedNodeIndex: number;
  layout?: WorkflowMonitorLayout;
  queuedMessages?: string[];
  pendingSteerContent?: string;
  approval?: ApprovalRequestInfo;
}

interface StepOptions {
  completionSignal?: WorkflowMonitorNode['completionSignal'];
  depth?: number;
  failureReason?: string;
  parentId?: string | null;
}

function step(
  workflowId: string,
  id: string,
  status: WorkflowNodeStatus,
  options: StepOptions = {}
): WorkflowMonitorNode {
  return {
    id,
    type: 'step',
    status,
    label: id,
    parentId: options.parentId ?? null,
    depth: options.depth ?? 0,
    nodePath: [id],
    sessionId: `${workflowId}:${id}`,
    agentName: id.includes('review') ? 'wf-reviewer' : 'wf-coder',
    completionSignal: options.completionSignal,
    failureReason: options.failureReason,
  };
}

function run(
  workflowId: string,
  name: string,
  status: WorkflowStatus,
  nodes: WorkflowMonitorNode[],
  elapsedSeconds: number,
  pauseReason?: string
): WorkflowRunView {
  return {
    workflowId,
    parentSessionId: 'storybook-parent-session',
    name,
    status,
    nodes,
    stepSessions: nodes.flatMap((node) =>
      node.type === 'step' && node.sessionId
        ? [
            {
              nodeId: node.id,
              nodePath: node.nodePath ?? [node.id],
              sessionId: node.sessionId,
              status: node.status,
              agentName: node.agentName,
            },
          ]
        : []
    ),
    startedAt: STORY_NOW - elapsedSeconds * 1000,
    completedAt:
      status === 'completed' || status === 'failed' || status === 'aborted'
        ? STORY_NOW - 4000
        : null,
    pauseReason,
  };
}

function runningNodes(workflowId: string): WorkflowMonitorNode[] {
  return [
    {
      id: 'release-checks',
      type: 'parallel',
      status: 'running',
      label: 'release-checks',
      parentId: null,
      depth: 0,
    },
    step(workflowId, 'contract-review', 'completed', {
      parentId: 'release-checks',
      depth: 1,
    }),
    step(workflowId, 'workflow-ux', 'running', {
      parentId: 'release-checks',
      depth: 1,
    }),
    step(workflowId, 'regression-suite', 'pending', {
      parentId: 'release-checks',
      depth: 1,
    }),
  ];
}

function approvalFor(sessionId: string): ApprovalRequestInfo {
  return {
    sessionId,
    originSessionId: sessionId,
    toolId: 'execute_bash',
    toolCall: {
      toolCallId: `${sessionId}:tool-1`,
      title: 'Shell',
      rawInput: { command: 'bun test --filter workflow-monitor' },
    },
    permissionOptions: [
      {
        kind: ApprovalOptionId.AllowOnce,
        name: 'Allow once',
        optionId: ApprovalOptionId.AllowOnce,
      },
      {
        kind: ApprovalOptionId.AllowAlways,
        name: 'Always allow',
        optionId: ApprovalOptionId.AllowAlways,
      },
      {
        kind: ApprovalOptionId.RejectOnce,
        name: 'Reject once',
        optionId: ApprovalOptionId.RejectOnce,
      },
    ],
    consentContext: {
      capability: 'shell',
      resource: 'bun test --filter workflow-monitor',
    },
    resolve: () => {},
  };
}

function fixtureFor(scenario: WorkflowMonitorScenario): WorkflowFixture {
  const primaryId = `visual-${scenario}`;
  switch (scenario) {
    case 'running':
    case 'narrow':
    case 'stop-confirmation':
    case 'mouse-mode':
      return {
        workflows: [
          run(
            primaryId,
            'release-hardening',
            'running',
            runningNodes(primaryId),
            84
          ),
        ],
        activeWorkflowId: primaryId,
        selectedNodeIndex: 2,
      };
    case 'approval': {
      const nodes = runningNodes(primaryId);
      const selectedNode = nodes[2];
      if (!selectedNode?.sessionId) {
        throw new Error('Approval fixture requires a selected step session');
      }
      return {
        workflows: [run(primaryId, 'release-hardening', 'running', nodes, 84)],
        activeWorkflowId: primaryId,
        selectedNodeIndex: 2,
        approval: approvalFor(selectedNode.sessionId),
      };
    }
    case 'stacked':
      return {
        workflows: [
          run(
            primaryId,
            'stacked-validation',
            'running',
            runningNodes(primaryId),
            126
          ),
        ],
        activeWorkflowId: primaryId,
        selectedNodeIndex: 2,
        layout: 'stacked',
      };
    case 'need-input': {
      const node = step(primaryId, 'release-decision', 'paused', {
        completionSignal: 'need_input',
      });
      node.pauseReason = 'Choose whether to publish the release candidate.';
      return {
        workflows: [
          run(
            primaryId,
            'release-gate',
            'paused',
            [node],
            201,
            'Waiting for a release decision'
          ),
        ],
        activeWorkflowId: primaryId,
        selectedNodeIndex: 0,
      };
    }
    case 'completed':
      return {
        workflows: [
          run(
            primaryId,
            'two-wave-smoke',
            'completed',
            [
              step(primaryId, 'wave-1', 'completed'),
              step(primaryId, 'wave-2', 'completed'),
            ],
            184
          ),
        ],
        activeWorkflowId: primaryId,
        selectedNodeIndex: 1,
      };
    case 'failed':
      return {
        workflows: [
          run(
            primaryId,
            'topology-validation',
            'failed',
            [
              step(primaryId, 'fan-out', 'completed'),
              step(primaryId, 'stop-condition', 'failed', {
                failureReason: 'Stop condition was not satisfied.',
              }),
            ],
            97
          ),
        ],
        activeWorkflowId: primaryId,
        selectedNodeIndex: 1,
      };
    case 'multi-workflow': {
      const pausedId = 'visual-paused';
      const completedId = 'visual-completed';
      return {
        workflows: [
          run(
            primaryId,
            'release-hardening',
            'running',
            runningNodes(primaryId),
            84
          ),
          run(
            pausedId,
            'security-review',
            'paused',
            [
              step(pausedId, 'security-review', 'paused', {
                completionSignal: 'need_input',
              }),
            ],
            143,
            'Reviewer requested clarification'
          ),
          run(
            completedId,
            'smoke-baseline',
            'completed',
            [step(completedId, 'smoke-baseline', 'completed')],
            312
          ),
        ],
        activeWorkflowId: primaryId,
        selectedNodeIndex: 2,
        queuedMessages: [
          'Re-run the focused tests',
          'Compare the final frames',
        ],
        pendingSteerContent: 'Keep the report concise',
      };
    }
  }
}

function sessionStatus(status: WorkflowNodeStatus): SessionStatus {
  switch (status) {
    case 'running':
      return 'busy';
    case 'failed':
    case 'aborted':
      return 'failed';
    case 'pending':
    case 'skipped':
      return 'pending';
    case 'paused':
      return 'idle';
    case 'completed':
      return 'terminated';
  }
}

function seedConversation(
  session: AgentSession,
  node: WorkflowMonitorNode
): (event: AgentStreamEvent) => void {
  const conversations = sessionConversationsStore.getState();
  conversations.clearSession(session.id);
  const handle = conversations.createHandlerForSession(session.id);
  const events: AgentStreamEvent[] = [
    {
      type: AgentEventType.Content,
      id: `${session.id}:content-1`,
      content: {
        type: ContentType.Text,
        text: `Inspecting ${node.label} against the release contract.`,
      },
    },
    {
      type: AgentEventType.ToolCall,
      id: `${session.id}:tool-1`,
      name: 'Shell',
      kind: 'shell',
      args: { command: 'bun test --filter workflow-monitor' },
      locations: [],
    },
    {
      type: AgentEventType.ToolCallFinished,
      id: `${session.id}:tool-1`,
      result:
        node.status === 'failed'
          ? {
              status: 'error',
              error: node.failureReason ?? 'Validation failed',
            }
          : { status: 'success', output: '12 passed, 0 failed' },
    },
    {
      type: AgentEventType.Content,
      id: `${session.id}:content-2`,
      content: {
        type: ContentType.Text,
        text:
          node.status === 'failed'
            ? `Blocked: ${node.failureReason ?? 'Validation failed'}`
            : node.status === 'paused'
              ? 'Evidence is ready. A release decision is required.'
              : 'Focused checks passed. The workflow transcript remains isolated.',
      },
    },
  ];
  events.forEach(handle);
  return handle;
}

type StoryWorkflowMessageSender = (
  target: WorkflowNodeSessionTarget,
  content: string
) => Promise<void>;

class StoryKiro extends Kiro {
  constructor(
    private readonly sendWorkflowMessage: StoryWorkflowMessageSender
  ) {
    super();
  }

  override messageWorkflowNode(
    target: WorkflowNodeSessionTarget,
    content: string
  ): Promise<void> {
    return this.sendWorkflowMessage(target, content);
  }
}

function createStoryStores(fixture: WorkflowFixture): {
  appStore: AppStoreApi;
  monitorStore: StoreApi<WorkflowStoreState>;
} {
  const monitorStore = createWorkflowStore(storyNow);
  fixture.workflows.forEach((workflow) =>
    monitorStore.getState().openHistoricalWorkflow(workflow)
  );
  monitorStore.setState((state) => ({
    activeWorkflowId: fixture.activeWorkflowId,
    monitorLayout: fixture.layout ?? 'side-by-side',
    selectedNodeIndices: new Map(state.selectedNodeIndices).set(
      fixture.activeWorkflowId,
      fixture.selectedNodeIndex
    ),
  }));

  const conversationHandlers = new Map<
    string,
    (event: AgentStreamEvent) => void
  >();
  const nodeBySessionId = new Map(
    fixture.workflows.flatMap((workflow) =>
      workflow.nodes.flatMap((node) =>
        node.sessionId ? [[node.sessionId, node] as const] : []
      )
    )
  );
  const kiro = new StoryKiro(async (target, content) => {
    const handler = conversationHandlers.get(target.sessionId);
    const node = nodeBySessionId.get(target.sessionId);
    if (!handler || !node) {
      throw new Error('Story workflow session is not registered');
    }
    const delivery = node.status === 'running' ? 'steer' : 'prompt';
    if (delivery === 'steer') {
      handler({
        type: AgentEventType.SteeringConsumed,
        content,
      });
    } else {
      handler({
        type: AgentEventType.UserMessage,
        id: `${target.sessionId}:follow-up`,
        content: { type: ContentType.Text, text: content },
      });
    }
    setTimeout(() => {
      handler({
        type: AgentEventType.Thought,
        id: `${target.sessionId}:follow-up-thinking`,
        content: {
          type: ContentType.Text,
          text: 'Re-evaluating the focused evidence.',
        },
      });
    }, 1000);
    setTimeout(() => {
      handler({
        type: AgentEventType.Content,
        id: `${target.sessionId}:follow-up-response`,
        content: {
          type: ContentType.Text,
          text: 'Steering applied. The focused contract still passes.',
        },
      });
    }, 2000);
  });
  const appStore = createAppStore({
    kiro,
    agentEngine: 'kas',
  });
  appStore.setState({
    queuedMessages: fixture.queuedMessages ?? [],
    pendingSteerContent: fixture.pendingSteerContent ?? null,
    approvalQueue: fixture.approval ? [fixture.approval] : [],
  });
  fixture.workflows.forEach((workflow) => {
    workflow.nodes.forEach((node) => {
      if (
        node.type !== 'step' ||
        !node.sessionId ||
        !workflow.parentSessionId
      ) {
        return;
      }
      const session: AgentSession = {
        id: node.sessionId,
        name: node.agentName ?? node.label,
        agentName: node.agentName,
        status: sessionStatus(node.status),
        type: 'ephemeral',
        group: workflow.workflowId,
        parentSession: workflow.parentSessionId,
        lifecycleOwner: SessionLifecycleOwner.WorkflowExtension,
        created: new Date(STORY_NOW - 120_000),
        lastActivity: new Date(STORY_NOW - 2_000),
      };
      appStore.getState().addSession(session);
      conversationHandlers.set(session.id, seedConversation(session, node));
    });
  });

  return { appStore, monitorStore };
}

function WorkflowMonitorStory({
  scenario,
}: WorkflowMonitorStoryProps): React.ReactElement {
  const [stores] = useState(() => createStoryStores(fixtureFor(scenario)));
  return (
    <AppStoreContext.Provider value={stores.appStore}>
      <WorkflowMonitorScreen store={stores.monitorStore} now={storyNow} />
    </AppStoreContext.Provider>
  );
}

function certification(
  assertions: NonNullable<
    NonNullable<StorybookParameters['certification']>['assertions']
  >,
  viewport?: { columns: number; rows: number }
): StorybookParameters {
  return {
    layout: 'fullscreen',
    capturesKeyboard: true,
    certification: {
      suite: 'workflow-monitor',
      readyText: 'WORKFLOWS',
      assertions: {
        visible: ['Steps', 'Output', ...(assertions.visible ?? [])],
        hidden: [
          'DYNAMIC WORKFLOWS',
          'SUBAGENT OUTPUT',
          'undefined',
          ...(assertions.hidden ?? []),
        ],
      },
      ...(viewport ? { viewport } : {}),
    },
  };
}

function openComposer(label: string): StorybookPlay {
  return async ({ type, waitFor }) => {
    await type('s');
    await waitFor(label);
  };
}

const navigateToNextWorkflow: StorybookPlay = async ({ press, waitFor }) => {
  await press('right');
  await waitFor('security-review - paused');
};

const armStopConfirmation: StorybookPlay = async ({ press, waitFor }) => {
  await press('ctrl+x');
  await waitFor('ctrl+x stop workflow');
};

const enableMouseMode: StorybookPlay = async ({ type, waitFor }) => {
  await type('m');
  await waitFor('m mouse:on');
};

const STEER_MESSAGE = 'Prioritize the protocol-boundary evidence.';
const FOLLOW_UP_RESPONSE =
  'Steering applied. The focused contract still passes.';

const captureSteerConversation: StorybookPlay = async ({
  type,
  press,
  waitFor,
  capture,
}) => {
  await type('s');
  await waitFor('Steer');
  await type(STEER_MESSAGE, { delayMs: 0 });
  await press('enter');
  await waitFor(STEER_MESSAGE);
  await capture('steer sent in workflow transcript');
  await waitFor('Thinking...');
  await capture('workflow thinking after steer');
  await waitFor(FOLLOW_UP_RESPONSE);
  await capture('reply with preserved conversation history');
};

const captureConversationAfterWorkflowTabs: StorybookPlay = async ({
  type,
  press,
  waitFor,
  capture,
}) => {
  await type('s');
  await waitFor('Steer');
  await type(STEER_MESSAGE, { delayMs: 0 });
  await press('enter');
  await waitFor(FOLLOW_UP_RESPONSE);
  await press('right');
  await waitFor('security-review - paused');
  await press('left');
  await waitFor('release-hardening - running');
  await waitFor(STEER_MESSAGE);
  await capture('conversation preserved across workflow tabs');
};

const meta = {
  title: 'Workflows/WorkflowMonitor',
  component: WorkflowMonitorStory,
  parameters: {
    layout: 'fullscreen',
    storyOrder: [
      'Running',
      'RunningSteerOpen',
      'NeedsInputRespondOpen',
      'CompletedMessageOpen',
      'ApprovalInsideOutput',
      'SteerConversationLifecycle',
      'Failed',
      'MultiWorkflow',
      'MultiWorkflowNavigate',
      'MultiWorkflowConversationPreserved',
      'Stacked',
      'Narrow',
      'StopConfirmation',
      'MouseMode',
    ],
  },
};

export default meta;

export const Running = {
  args: { scenario: 'running' satisfies WorkflowMonitorScenario },
  parameters: certification({
    visible: [
      'release-hardening - running',
      'workflow-ux',
      'WORKFLOW OUTPUT',
      's steer',
    ],
  }),
};

export const RunningSteerOpen = {
  args: { scenario: 'running' satisfies WorkflowMonitorScenario },
  parameters: certification({
    visible: ['Steer', 'workflow-ux', 'send', 'esc close'],
  }),
  play: openComposer('Steer'),
};

export const NeedsInputRespondOpen = {
  args: { scenario: 'need-input' satisfies WorkflowMonitorScenario },
  parameters: certification({
    visible: ['release-gate - paused', 'Respond', 'release-decision'],
  }),
  play: openComposer('Respond'),
};

export const CompletedMessageOpen = {
  args: { scenario: 'completed' satisfies WorkflowMonitorScenario },
  parameters: certification({
    visible: ['two-wave-smoke - completed', 'Message', 'wave-2'],
  }),
  play: openComposer('Message'),
};

export const ApprovalInsideOutput = {
  args: { scenario: 'approval' satisfies WorkflowMonitorScenario },
  parameters: certification({
    visible: [
      'WORKFLOW OUTPUT',
      'Shell requires approval',
      'bun test --filter workflow-monitor',
      'Yes, single permission',
    ],
  }),
};

export const SteerConversationLifecycle = {
  args: { scenario: 'running' satisfies WorkflowMonitorScenario },
  parameters: certification({
    visible: ['WORKFLOW OUTPUT', STEER_MESSAGE],
  }),
  play: captureSteerConversation,
};

export const Failed = {
  args: { scenario: 'failed' satisfies WorkflowMonitorScenario },
  parameters: certification({
    visible: ['topology-validation - failed', 'stop-condition', 'Blocked:'],
    hidden: ['s steer', 's message'],
  }),
};

export const MultiWorkflow = {
  args: { scenario: 'multi-workflow' satisfies WorkflowMonitorScenario },
  parameters: certification({
    visible: [
      'release-hardening - running',
      'security-review',
      'smoke-baseline',
      '3 messages queued',
      '1-9 jump',
    ],
  }),
};

export const MultiWorkflowNavigate = {
  args: { scenario: 'multi-workflow' satisfies WorkflowMonitorScenario },
  parameters: certification({
    visible: ['security-review - paused', 'Reviewer requested clarification'],
  }),
  play: navigateToNextWorkflow,
};

export const MultiWorkflowConversationPreserved = {
  args: { scenario: 'multi-workflow' satisfies WorkflowMonitorScenario },
  parameters: certification({
    visible: ['release-hardening - running', STEER_MESSAGE, FOLLOW_UP_RESPONSE],
  }),
  play: captureConversationAfterWorkflowTabs,
};

export const Stacked = {
  args: { scenario: 'stacked' satisfies WorkflowMonitorScenario },
  parameters: certification({
    visible: ['stacked-validation - running', 'l split', 'WORKFLOW OUTPUT'],
  }),
};

export const Narrow = {
  args: { scenario: 'narrow' satisfies WorkflowMonitorScenario },
  parameters: certification(
    {
      visible: ['release-hardening - running', 'workflow-ux'],
    },
    { columns: 84, rows: 28 }
  ),
};

export const StopConfirmation = {
  args: { scenario: 'stop-confirmation' satisfies WorkflowMonitorScenario },
  parameters: certification({
    visible: ['ctrl+x stop workflow', 'esc keep running'],
  }),
  play: armStopConfirmation,
};

export const MouseMode = {
  args: { scenario: 'mouse-mode' satisfies WorkflowMonitorScenario },
  parameters: certification({
    visible: ['MOUSE ON', 'm mouse:on'],
  }),
  play: enableMouseMode,
};
