import React, { useState } from 'react';
import { useInput } from '../../renderer.js';
import { Kiro } from '../../kiro.js';
import {
  AppStoreContext,
  createAppStore,
  MessageRole,
  ToolUseStatus,
  type AppStoreApi,
  type MessageType,
} from '../../stores/app-store.js';
import { sessionConversationsStore } from '../../stores/session-conversations.js';
import type {
  StorybookParameters,
  StorybookPlay,
} from '../../storybook/contracts.js';
import { ThemeProvider } from '../../theme/index.js';
import {
  AgentEventType,
  ApprovalOptionId,
  ContentType,
  type ApprovalRequestInfo,
  type AgentStreamEvent,
} from '../../types/agent-events.js';
import type { AgentSession, SessionStatus } from '../../types/multi-session.js';
import { CrewMonitorScreen } from '../layout/CrewMonitorScreen.js';

type CrewMonitorScenario =
  | 'empty'
  | 'pending'
  | 'completed'
  | 'thinking'
  | 'failed'
  | 'approval'
  | 'mixed'
  | 'loop-deduplication'
  | 'navigation'
  | 'kill-confirmation';

interface CrewMonitorStoryProps {
  scenario: CrewMonitorScenario;
}

interface CrewFixture {
  sessions: AgentSession[];
  conversations?: ReadonlyMap<string, MessageType[]>;
  events?: Readonly<Record<string, AgentStreamEvent[]>>;
  approvalQueue?: ApprovalRequestInfo[];
  focusedIndex?: number;
  agentEngine?: 'v2' | 'kas';
}

interface CrewStoryStores {
  appStore: AppStoreApi;
}

const firstAssistant =
  'Inventory complete: authentication and storage surfaces mapped.';
const secondAssistant =
  'Plan approved: validate the cache key before release rollout.';
const streamingTail =
  'Applying the cache validation while preserving the verified history';
const toolCommand = 'bun test ./visual_tests/cache-key.test.ts';
const toolResult = '4 cache-key checks passed';

function session(
  id: string,
  name: string,
  status: SessionStatus,
  overrides: Partial<AgentSession> = {}
): AgentSession {
  return {
    id,
    name,
    role: 'engineer',
    status,
    type: 'ephemeral',
    created: new Date(`2026-08-18T12:00:0${id.length % 10}.000Z`),
    lastActivity: new Date('2026-08-18T12:01:00.000Z'),
    ...overrides,
  };
}

function user(id: string, content: string): MessageType {
  return { id, role: MessageRole.User, content };
}

function assistant(id: string, content: string): MessageType {
  return { id, role: MessageRole.Model, content };
}

function approvalFixture(): CrewFixture {
  const approval: ApprovalRequestInfo = {
    sessionId: 'approval-worker',
    toolCall: {
      toolCallId: 'approval-shell',
      name: 'execute_bash',
      kind: 'execute',
      rawInput: { command: 'cargo test -p agent' },
    },
    permissionOptions: [
      {
        kind: ApprovalOptionId.AllowOnce,
        name: 'Allow once',
        optionId: ApprovalOptionId.AllowOnce,
      },
      {
        kind: ApprovalOptionId.RejectOnce,
        name: 'Reject',
        optionId: ApprovalOptionId.RejectOnce,
      },
    ],
    resolve: () => undefined,
  };
  const messages: MessageType[] = [
    user('approval-user', 'Run the targeted Rust suite.'),
    {
      id: 'approval-shell',
      role: MessageRole.ToolUse,
      name: 'execute_bash',
      kind: 'execute',
      content: JSON.stringify({ command: 'cargo test -p agent' }),
      status: ToolUseStatus.Pending,
    },
  ];
  return {
    sessions: [session('approval-worker', 'rust-verifier', 'busy')],
    conversations: new Map([['approval-worker', messages]]),
    events: {
      'approval-worker': [
        {
          type: AgentEventType.ToolCall,
          id: 'approval-shell',
          name: 'execute_bash',
          kind: 'execute',
          args: { command: 'cargo test -p agent' },
        },
      ],
    },
    approvalQueue: [approval],
  };
}

function fixtureFor(scenario: CrewMonitorScenario): CrewFixture {
  switch (scenario) {
    case 'empty':
      return { sessions: [] };
    case 'pending':
      return {
        sessions: [
          session('pending-review', 'release-review', 'pending', {
            dependsOn: ['build-assets'],
          }),
        ],
      };
    case 'completed':
      return {
        sessions: [
          session('completed-worker', 'artifact-builder', 'terminated', {
            summary: 'All platform artifacts are available.',
          }),
        ],
        conversations: new Map([
          [
            'completed-worker',
            [
              user('completed-user', 'Build all release artifacts.'),
              assistant(
                'completed-assistant',
                'Built Linux, macOS, and Windows release artifacts.'
              ),
            ],
          ],
        ]),
        events: {
          'completed-worker': [
            {
              type: AgentEventType.Content,
              id: 'completed-event',
              content: { type: ContentType.Text, text: 'done' },
            },
          ],
        },
      };
    case 'thinking':
      return {
        sessions: [session('thinking-worker', 'dependency-auditor', 'busy')],
      };
    case 'failed':
      return {
        sessions: [
          session('failed-worker', 'windows-packager', 'failed', {
            summary: 'Windows linker failed.',
          }),
        ],
        conversations: new Map([
          [
            'failed-worker',
            [
              user('failed-user', 'Package the Windows binary.'),
              assistant(
                'failed-assistant',
                'The linker could not resolve the embedded asset.'
              ),
            ],
          ],
        ]),
        events: {
          'failed-worker': [
            {
              type: AgentEventType.Content,
              id: 'failed-event',
              content: { type: ContentType.Text, text: 'failed' },
            },
          ],
        },
      };
    case 'approval':
      return approvalFixture();
    case 'mixed':
      return {
        sessions: [
          session('crew-research', 'research-contracts', 'terminated', {
            group: 'release-crew',
            role: 'researcher',
          }),
          session('crew-build', 'build-assets', 'busy', {
            group: 'release-crew',
            role: 'builder',
            dependsOn: ['research-contracts'],
          }),
          session('solo-docs', 'update-release-notes', 'busy', {
            role: 'writer',
          }),
        ],
      };
    case 'loop-deduplication':
      return {
        sessions: [
          session('loop-old', 'retry-validation', 'terminated', {
            group: 'release-loop',
            hasLoop: true,
            loopIteration: 0,
            loopMaxIterations: 3,
          }),
          session('loop-new', 'retry-validation', 'busy', {
            group: 'release-loop',
            hasLoop: true,
            loopIteration: 1,
            loopMaxIterations: 3,
          }),
        ],
        conversations: new Map([
          [
            'loop-old',
            [
              user('loop-old-user', 'Run the first attempt.'),
              assistant(
                'loop-old-assistant',
                'Obsolete first iteration output'
              ),
            ],
          ],
          [
            'loop-new',
            [
              user('loop-new-user', 'Retry with corrected inputs.'),
              assistant(
                'loop-new-assistant',
                'Current second iteration output'
              ),
            ],
          ],
        ]),
      };
    case 'navigation':
      return {
        sessions: [
          session('nav-one', 'source-auditor', 'terminated', {
            created: new Date('2026-08-18T12:00:00.000Z'),
          }),
          session('nav-two', 'binary-verifier', 'busy', {
            created: new Date('2026-08-18T12:01:00.000Z'),
          }),
        ],
        conversations: new Map([
          [
            'nav-one',
            [
              user('nav-one-user', 'Audit source inputs.'),
              assistant('nav-one-assistant', 'Source audit complete.'),
            ],
          ],
          [
            'nav-two',
            [
              user('nav-two-user', 'Verify the binary.'),
              assistant(
                'nav-two-assistant',
                'Binary verification in progress.'
              ),
            ],
          ],
        ]),
      };
    case 'kill-confirmation':
      return {
        sessions: [session('kill-worker', 'long-running-check', 'busy')],
        agentEngine: 'v2',
      };
  }
}

function applyFixture(stores: CrewStoryStores, fixture: CrewFixture): void {
  sessionConversationsStore.setState({
    conversations: new Map(fixture.conversations ?? []),
  });
  stores.appStore.setState({
    sessions: new Map(
      fixture.sessions.map((activeSession) => [activeSession.id, activeSession])
    ),
    sessionEventBuffer: { ...(fixture.events ?? {}) },
    approvalQueue: [...(fixture.approvalQueue ?? [])],
    pendingApproval: fixture.approvalQueue?.[0] ?? null,
    focusedCrewIndex: fixture.focusedIndex ?? 0,
    liveOutputs: new Map(),
  });
}

function createStoryStores(fixture: CrewFixture): CrewStoryStores {
  const appStore = createAppStore({
    kiro: new Kiro(),
    agentEngine: fixture.agentEngine ?? 'kas',
  });
  const stores = { appStore };
  applyFixture(stores, fixture);
  return stores;
}

function CrewStorySurface({
  stores,
}: {
  stores: CrewStoryStores;
}): React.ReactElement {
  return (
    <ThemeProvider>
      <AppStoreContext.Provider value={stores.appStore}>
        <CrewMonitorScreen />
      </AppStoreContext.Provider>
    </ThemeProvider>
  );
}

function CrewMonitorStory({
  scenario,
}: CrewMonitorStoryProps): React.ReactElement {
  const [stores] = useState(() => createStoryStores(fixtureFor(scenario)));
  return <CrewStorySurface stores={stores} />;
}

function CrewTransitionStory(): React.ReactElement {
  const [{ stores, handleEvent }] = useState(() => {
    const journeyStores = createStoryStores({ sessions: [] });
    const state = journeyStores.appStore.getState();
    state.addSession(session('journey-worker', 'cache-validator', 'idle'));
    const handler = sessionConversationsStore
      .getState()
      .createHandlerForSession('journey-worker');
    const emit = (event: AgentStreamEvent): void => {
      handler(event);
      journeyStores.appStore
        .getState()
        .pushSessionEvent('journey-worker', event);
    };
    emit({
      type: AgentEventType.UserMessage,
      id: 'user-1',
      content: {
        type: ContentType.Text,
        text: 'Map the release-sensitive surfaces.',
      },
    });
    emit({
      type: AgentEventType.Content,
      id: 'assistant-1',
      content: { type: ContentType.Text, text: firstAssistant },
    });
    emit({
      type: AgentEventType.UserMessage,
      id: 'user-2',
      content: {
        type: ContentType.Text,
        text: 'Choose a safe rollout plan.',
      },
    });
    emit({
      type: AgentEventType.Content,
      id: 'assistant-2',
      content: { type: ContentType.Text, text: secondAssistant },
    });
    return { stores: journeyStores, handleEvent: emit };
  });
  const [step, setStep] = useState(0);
  useInput((_, key) => {
    if (!key.tab || step >= 3) return;
    const nextStep = step + 1;
    if (nextStep === 1) {
      stores.appStore
        .getState()
        .updateSession('journey-worker', { status: 'busy' });
      handleEvent({
        type: AgentEventType.UserMessage,
        id: 'user-3',
        content: {
          type: ContentType.Text,
          text: 'Apply the plan and show progress.',
        },
      });
      handleEvent({
        type: AgentEventType.Content,
        id: 'assistant-3',
        content: { type: ContentType.Text, text: streamingTail },
      });
    } else if (nextStep === 2) {
      handleEvent({
        type: AgentEventType.ToolCall,
        id: 'journey-shell',
        name: 'execute_bash',
        kind: 'execute',
        args: { command: toolCommand },
      });
    } else {
      handleEvent({
        type: AgentEventType.ToolCallFinished,
        id: 'journey-shell',
        result: { status: 'success', output: toolResult },
      });
      stores.appStore.getState().updateSession('journey-worker', {
        status: 'terminated',
        summary: 'Cache validation completed safely.',
      });
    }
    setStep(nextStep);
  });
  return <CrewStorySurface stores={stores} />;
}

function certification(
  readyText: string,
  assertions: NonNullable<
    NonNullable<StorybookParameters['certification']>['assertions']
  >,
  coversVisualStates: readonly string[],
  captures?: NonNullable<StorybookParameters['certification']>['captures']
): StorybookParameters {
  return {
    layout: 'fullscreen',
    capturesKeyboard: true,
    coversVisualStates,
    certification: {
      suite: 'visual-stories',
      readyText,
      viewport: { columns: 150, rows: 38 },
      assertions: {
        visible: ['AGENT MONITOR', ...(assertions.visible ?? [])],
        hidden: ['undefined', ...(assertions.hidden ?? [])],
      },
      ...(captures ? { captures } : {}),
    },
  };
}

const captureConversationLifecycle: StorybookPlay = async ({
  press,
  waitFor,
  capture,
}) => {
  await capture('static-history');
  await press('tab');
  await waitFor(streamingTail);
  await capture('streaming-tail');
  await press('tab');
  await waitFor(toolCommand);
  await capture('tool-started');
  await press('tab');
  await waitFor('Summary: Cache validation completed safely.');
  await capture('tool-completed');
};

const captureSessionNavigation: StorybookPlay = async ({
  press,
  waitFor,
  capture,
}) => {
  await capture('first-session');
  await press('right');
  await waitFor('Binary verification in progress.');
  await capture('second-session');
};

const armKillConfirmation: StorybookPlay = async ({
  press,
  waitFor,
  capture,
}) => {
  await press('ctrl+x');
  await waitFor('Press ctrl+x again to kill session');
  await capture('kill-armed');
};

const meta = {
  title: 'Agents/CrewMonitor',
  component: CrewMonitorStory,
  parameters: {
    layout: 'fullscreen',
    visualStates: {
      empty: { label: 'No active subagents' },
      pending: { label: 'Pending session waiting on dependencies' },
      completed: {
        label: 'Completed session with retained output and summary',
      },
      thinking: { label: 'Executing session without an active tool' },
      failed: { label: 'Failed session with retained output' },
      approval: { label: 'Active tool approval inside selected session' },
      'mixed-crew-standalone': {
        label: 'Grouped crew and standalone sessions together',
        gapType: 'product-limitation',
        description:
          'CrewMonitor currently filters the DAG to the first group and omits standalone sessions whenever a group exists.',
      },
      'loop-deduplicated': {
        label: 'Only the latest loop iteration remains visible',
      },
      'session-navigation': {
        label: 'Selection moves between session outputs',
      },
      'kill-confirmation': { label: 'V2 session kill confirmation armed' },
      'static-history': { label: 'Two completed assistant turns' },
      'streaming-tail': {
        label: 'Two static turns plus one active assistant tail',
      },
      'tool-started': {
        label: 'Active tool follows and preserves assistant history',
      },
      'tool-output-streaming': {
        label: 'Live subagent tool output while the tool is running',
        gapType: 'product-limitation',
        description:
          'The multi-session conversation path drops ToolCallUpdate and does not populate the liveOutputs map consumed by Shell.',
      },
      'tool-completed': {
        label: 'Completed tool result with prior assistant history preserved',
      },
      'tool-result-body': {
        label: 'Completed subagent tool result body',
        gapType: 'product-limitation',
        description:
          'Legacy static Shell rendering hides result output in SessionOutput after completion.',
      },
    },
    storyOrder: [
      'Empty',
      'PendingOnly',
      'CompletedHistory',
      'Thinking',
      'Failed',
      'ApprovalRequired',
      'MixedCrewAndStandalone',
      'LoopIterationDeduplication',
      'SessionNavigation',
      'ConversationAndToolLifecycle',
      'KillConfirmation',
    ],
  },
};

export default meta;

export const Empty = {
  args: { scenario: 'empty' satisfies CrewMonitorScenario },
  parameters: certification(
    'No active subagents',
    { visible: ['No active subagents', 'q or ctrl+g to return to chat'] },
    ['empty']
  ),
};

export const PendingOnly = {
  args: { scenario: 'pending' satisfies CrewMonitorScenario },
  parameters: certification(
    'release-review',
    {
      visible: ['release-review', 'Waiting', 'No activity yet'],
      hidden: ['Thinking...'],
    },
    ['pending']
  ),
};

export const CompletedHistory = {
  args: { scenario: 'completed' satisfies CrewMonitorScenario },
  parameters: certification(
    'Built Linux, macOS, and Windows release artifacts.',
    {
      visible: [
        'artifact-builder',
        'Completed',
        'Built Linux, macOS, and Windows release artifacts.',
        'Summary: All platform artifacts are available.',
      ],
    },
    ['completed']
  ),
};

export const Thinking = {
  args: { scenario: 'thinking' satisfies CrewMonitorScenario },
  parameters: certification(
    'Thinking...',
    { visible: ['dependency-auditor', 'Thinking...'] },
    ['thinking']
  ),
};

export const Failed = {
  args: { scenario: 'failed' satisfies CrewMonitorScenario },
  parameters: certification(
    'The linker could not resolve the embedded asset.',
    {
      visible: [
        'windows-packager',
        'Failed',
        'The linker could not resolve the embedded asset.',
      ],
    },
    ['failed']
  ),
};

export const ApprovalRequired = {
  args: { scenario: 'approval' satisfies CrewMonitorScenario },
  parameters: certification(
    'Yes, single permission',
    {
      visible: [
        'rust-verifier',
        'cargo test -p agent',
        'Yes, single permission',
        'No',
      ],
    },
    ['approval']
  ),
};

export const MixedCrewAndStandalone = {
  args: { scenario: 'mixed' satisfies CrewMonitorScenario },
  parameters: certification(
    'research-contracts',
    {
      visible: [
        'research-contracts',
        'build-assets',
        'Completed',
        'Thinking...',
      ],
    },
    []
  ),
};

export const LoopIterationDeduplication = {
  args: {
    scenario: 'loop-deduplication' satisfies CrewMonitorScenario,
  },
  parameters: certification(
    'Current second iteration output',
    {
      visible: ['retry-validation', '[2/3]', 'Current second iteration output'],
      hidden: ['Obsolete first iteration output'],
    },
    ['loop-deduplicated']
  ),
};

export const SessionNavigation = {
  args: { scenario: 'navigation' satisfies CrewMonitorScenario },
  parameters: certification(
    'Source audit complete.',
    { visible: ['source-auditor', 'binary-verifier'] },
    [],
    {
      'first-session': {
        label: 'first session output selected',
        assertions: { visible: ['Source audit complete.'] },
      },
      'second-session': {
        label: 'second session output selected',
        assertions: { visible: ['Binary verification in progress.'] },
        coversVisualStates: ['session-navigation'],
      },
    }
  ),
  play: captureSessionNavigation,
};

export const ConversationAndToolLifecycle = {
  component: CrewTransitionStory,
  parameters: certification(
    firstAssistant,
    { visible: [firstAssistant, secondAssistant] },
    [],
    {
      'static-history': {
        label: 'two static assistant turns',
        coversVisualStates: ['static-history'],
        assertions: {
          visible: [firstAssistant, secondAssistant],
          hidden: [streamingTail, toolCommand],
          ordered: [firstAssistant, secondAssistant],
          occurrences: { [firstAssistant]: 1, [secondAssistant]: 1 },
        },
      },
      'streaming-tail': {
        label: 'streaming tail with static history preserved',
        coversVisualStates: ['streaming-tail'],
        assertions: {
          visible: [firstAssistant, secondAssistant, streamingTail],
          hidden: [toolCommand],
          ordered: [firstAssistant, secondAssistant, streamingTail],
          occurrences: {
            [firstAssistant]: 1,
            [secondAssistant]: 1,
            [streamingTail]: 1,
          },
        },
      },
      'tool-started': {
        label: 'active tool with assistant history preserved',
        coversVisualStates: ['tool-started'],
        assertions: {
          visible: [
            firstAssistant,
            secondAssistant,
            streamingTail,
            toolCommand,
          ],
          hidden: [toolResult],
          ordered: [
            firstAssistant,
            secondAssistant,
            streamingTail,
            toolCommand,
          ],
          occurrences: {
            [firstAssistant]: 1,
            [secondAssistant]: 1,
            [streamingTail]: 1,
            [toolCommand]: 1,
          },
        },
      },
      'tool-completed': {
        label: 'completed tool with assistant history preserved',
        coversVisualStates: ['tool-completed'],
        assertions: {
          visible: [
            firstAssistant,
            secondAssistant,
            streamingTail,
            toolCommand,
            'Summary: Cache validation completed safely.',
          ],
          hidden: ['esc to cancel'],
          ordered: [
            firstAssistant,
            secondAssistant,
            streamingTail,
            toolCommand,
            'Summary: Cache validation completed safely.',
          ],
          occurrences: {
            [firstAssistant]: 1,
            [secondAssistant]: 1,
            [streamingTail]: 1,
            [toolCommand]: 1,
          },
        },
      },
    }
  ),
  play: captureConversationLifecycle,
};

export const KillConfirmation = {
  args: {
    scenario: 'kill-confirmation' satisfies CrewMonitorScenario,
  },
  parameters: certification(
    'long-running-check',
    { visible: ['ctrl+x kill session'] },
    [],
    {
      'kill-armed': {
        label: 'kill confirmation armed',
        coversVisualStates: ['kill-confirmation'],
        assertions: {
          visible: ['Press ctrl+x again to kill session'],
        },
      },
    }
  ),
  play: armKillConfirmation,
};
