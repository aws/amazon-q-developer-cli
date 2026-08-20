import React, { useState } from 'react';
import { Kiro } from '../../../kiro.js';
import {
  AppStoreContext,
  createAppStore,
  type AppStoreApi,
} from '../../../stores/app-store.js';
import { workflowStore } from '../../../stores/workflow-store.js';
import type {
  StorybookParameters,
  StorybookPlay,
} from '../../../storybook/contracts.js';
import type { TaskItem } from '../../../types/tasks.js';
import type {
  WorkflowMonitorNode,
  WorkflowRunView,
} from '../../../types/workflow-monitor.js';
import type { WorkflowStatus } from '../../../types/workflow.js';
import { ActivityTray } from './ActivityTray.js';

type ActivityTrayScenario =
  | 'collapsed-workflows'
  | 'collapsed-tasks-remaining'
  | 'collapsed-tasks-done'
  | 'collapsed-queue'
  | 'collapsed-mixed'
  | 'expanded-tasks-mixed'
  | 'expanded-tasks-scrolled'
  | 'expanded-queue'
  | 'expanded-queue-editing'
  | 'expanded-queue-scrolled'
  | 'expanded-steer-only'
  | 'expanded-queue-and-steer'
  | 'expanded-mixed-tabs'
  | 'expanded-workflow'
  | 'expanded-completed-workflow';

interface ActivityTrayStoryProps {
  scenario: ActivityTrayScenario;
}

interface ActivityTrayFixture {
  display: 'collapsed' | 'expanded';
  tasks: TaskItem[];
  queuedMessages: string[];
  pendingSteerContent: string | null;
  editingQueueIndex: number | null;
  selectedIndex: number;
  activeTab: 'tasks' | 'queue' | 'workflow' | null;
  workflows: WorkflowRunView[];
}

interface ActivityTrayStoryStores {
  appStore: AppStoreApi;
}

const mixedTasks: TaskItem[] = [
  { id: '1', subject: 'Inspect the release queue', status: 'completed' },
  { id: '2', subject: 'Validate Windows packaging', status: 'pending' },
  { id: '3', subject: 'Publish verification evidence', status: 'pending' },
];

const completedTasks: TaskItem[] = [
  { id: '1', subject: 'Compile the release binary', status: 'completed' },
  { id: '2', subject: 'Run smoke certification', status: 'completed' },
];

const scrolledTasks: TaskItem[] = Array.from({ length: 9 }, (_, index) => ({
  id: `${index + 1}`,
  subject: `Release task ${index + 1}`,
  status: index < 5 ? ('completed' as const) : ('pending' as const),
}));

const queuedMessages = [
  'Review the release report',
  'Verify the Windows artifact',
  'Check terminal contrast',
];

const longQueue = Array.from(
  { length: 9 },
  (_, index) => `Queued release message ${index + 1}`
);

function workflowStep(
  workflowId: string,
  id: string,
  status: WorkflowMonitorNode['status']
): WorkflowMonitorNode {
  return {
    id,
    type: 'step',
    status,
    label: id,
    parentId: null,
    depth: 0,
    nodePath: [id],
    sessionId: `${workflowId}:${id}`,
    agentName: id,
  };
}

function workflowRun(
  workflowId: string,
  name: string,
  status: WorkflowStatus,
  order: number
): WorkflowRunView {
  const finalStatus: WorkflowMonitorNode['status'] =
    status === 'completed'
      ? 'completed'
      : status === 'paused'
        ? 'paused'
        : 'running';
  return {
    workflowId,
    parentSessionId: 'tray-story-parent',
    name,
    status,
    nodes: [
      workflowStep(workflowId, 'contract-check', 'completed'),
      workflowStep(workflowId, 'visual-check', finalStatus),
    ],
    stepSessions: [],
    startedAt: Date.parse('2026-07-20T17:00:00.000Z') - order * 1000,
    completedAt:
      status === 'completed' ? Date.parse('2026-07-20T17:10:00.000Z') : null,
  };
}

const liveWorkflows = [
  workflowRun('tray-1', 'release-hardening', 'running', 1),
  workflowRun('tray-2', 'security-review', 'paused', 2),
  workflowRun('tray-3', 'smoke-matrix', 'running', 3),
  workflowRun('tray-4', 'docs-check', 'paused', 4),
  workflowRun('tray-5', 'packaging', 'running', 5),
];

function emptyFixture(): ActivityTrayFixture {
  return {
    display: 'collapsed',
    tasks: [],
    queuedMessages: [],
    pendingSteerContent: null,
    editingQueueIndex: null,
    selectedIndex: 0,
    activeTab: null,
    workflows: [],
  };
}

function fixtureFor(scenario: ActivityTrayScenario): ActivityTrayFixture {
  const fixture = emptyFixture();
  switch (scenario) {
    case 'collapsed-workflows':
      return { ...fixture, workflows: liveWorkflows };
    case 'collapsed-tasks-remaining':
      return { ...fixture, tasks: mixedTasks };
    case 'collapsed-tasks-done':
      return { ...fixture, tasks: completedTasks };
    case 'collapsed-queue':
      return { ...fixture, queuedMessages };
    case 'collapsed-mixed':
      return {
        ...fixture,
        tasks: mixedTasks,
        queuedMessages: queuedMessages.slice(0, 2),
        workflows: liveWorkflows.slice(0, 1),
      };
    case 'expanded-tasks-mixed':
      return {
        ...fixture,
        display: 'expanded',
        tasks: mixedTasks,
        activeTab: 'tasks',
      };
    case 'expanded-tasks-scrolled':
      return {
        ...fixture,
        display: 'expanded',
        tasks: scrolledTasks,
        activeTab: 'tasks',
      };
    case 'expanded-queue':
      return {
        ...fixture,
        display: 'expanded',
        queuedMessages,
        selectedIndex: 1,
        activeTab: 'queue',
      };
    case 'expanded-queue-editing':
      return {
        ...fixture,
        display: 'expanded',
        queuedMessages,
        editingQueueIndex: 1,
        selectedIndex: 1,
        activeTab: 'queue',
      };
    case 'expanded-queue-scrolled':
      return {
        ...fixture,
        display: 'expanded',
        queuedMessages: longQueue,
        selectedIndex: 7,
        activeTab: 'queue',
      };
    case 'expanded-steer-only':
      return {
        ...fixture,
        display: 'expanded',
        pendingSteerContent: 'Redirect the active turn to release validation',
        activeTab: 'queue',
      };
    case 'expanded-queue-and-steer':
      return {
        ...fixture,
        display: 'expanded',
        queuedMessages: queuedMessages.slice(0, 2),
        pendingSteerContent: 'Prioritize the active release validation',
        selectedIndex: 1,
        activeTab: 'queue',
      };
    case 'expanded-mixed-tabs':
      return {
        ...fixture,
        display: 'expanded',
        tasks: mixedTasks,
        queuedMessages,
        activeTab: 'workflow',
        workflows: liveWorkflows.slice(0, 2),
      };
    case 'expanded-workflow':
      return {
        ...fixture,
        display: 'expanded',
        activeTab: 'workflow',
        workflows: liveWorkflows.slice(0, 2),
      };
    case 'expanded-completed-workflow':
      return {
        ...fixture,
        display: 'expanded',
        activeTab: 'workflow',
        workflows: [
          workflowRun('tray-complete', 'release-certified', 'completed', 1),
        ],
      };
  }
}

function createStoryStores(
  fixture: ActivityTrayFixture
): ActivityTrayStoryStores {
  workflowStore.getState().reset();
  fixture.workflows.forEach((run) =>
    workflowStore.getState().openHistoricalWorkflow(run)
  );
  const activeWorkflow = fixture.workflows[0];
  if (activeWorkflow) {
    workflowStore.getState().setActiveWorkflow(activeWorkflow.workflowId);
  }

  const appStore = createAppStore({
    kiro: new Kiro(),
    agentEngine: 'kas',
  });
  appStore.setState({
    tasks: fixture.tasks,
    queuedMessages: fixture.queuedMessages,
    pendingSteerContent: fixture.pendingSteerContent,
    editingQueueIndex: fixture.editingQueueIndex,
    activityTraySelectedIndex: fixture.selectedIndex,
    activityTrayTab: fixture.activeTab,
    activityTrayExpanded: fixture.display === 'expanded',
  });
  return { appStore };
}

function ActivityTrayStory({
  scenario,
}: ActivityTrayStoryProps): React.ReactElement {
  const fixture = fixtureFor(scenario);
  const [stores] = useState(() => createStoryStores(fixture));

  return (
    <AppStoreContext.Provider value={stores.appStore}>
      <ActivityTray />
    </AppStoreContext.Provider>
  );
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
      suite: 'workflow-monitor',
      readyText,
      viewport: { columns: 150, rows: 16 },
      assertions: {
        hidden: ['undefined', ...(assertions.hidden ?? [])],
        visible: assertions.visible,
      },
      ...(captures ? { captures } : {}),
    },
  };
}

const captureMixedTabs: StorybookPlay = async ({ press, waitFor, capture }) => {
  await capture('workflow');
  await press('tab');
  await waitFor('Tasks (3)');
  await capture('tasks');
  await press('tab');
  await waitFor('Messages (3)');
  await capture('queue');
};

const captureWorkflowNavigation: StorybookPlay = async ({
  press,
  waitFor,
  capture,
}) => {
  await capture('first-workflow');
  await press('shift+down');
  await waitFor('❯ └── ◐ visual-check');
  await capture('selected-node');
  await press('shift+right');
  await waitFor('◐ security-review (1/2)');
  await waitFor('❯ ├── ✓ contract-check');
  await capture('next-workflow');
};

const captureExpandCollapse: StorybookPlay = async ({
  press,
  waitFor,
  capture,
}) => {
  await capture('collapsed');
  await press('ctrl+x');
  await waitFor('Tasks (3)');
  await capture('expanded');
  await press('ctrl+x');
  await waitFor('2 messages queued');
  await capture('collapsed-again');
};

const meta = {
  title: 'Workflows/ActivityTray',
  component: ActivityTrayStory,
  parameters: {
    layout: 'fullscreen',
    visualStates: {
      'collapsed-workflow-overflow': {
        label: 'Collapsed workflow summary with overflow',
      },
      'collapsed-tasks-remaining': {
        label: 'Collapsed tasks with remaining work',
      },
      'collapsed-tasks-complete': {
        label: 'Collapsed tasks with all work complete',
      },
      'collapsed-queue': { label: 'Collapsed queued-message summary' },
      'collapsed-mixed': {
        label: 'Collapsed workflows, queue, and tasks together',
      },
      'tasks-mixed': { label: 'Expanded mixed task statuses' },
      'tasks-scrolled': { label: 'Expanded task list beyond six rows' },
      'queue-selected': { label: 'Expanded queue selected row' },
      'queue-editing': { label: 'Expanded queue editing row' },
      'queue-scrolled': { label: 'Expanded queue beyond six rows' },
      'steer-only': { label: 'Expanded backend-held steering row' },
      'queue-and-steer': {
        label: 'Steering row above numbered queued messages',
      },
      'mixed-tab-workflow': { label: 'Mixed tray on workflow tab' },
      'mixed-tab-tasks': { label: 'Mixed tray transitioned to tasks tab' },
      'mixed-tab-queue': { label: 'Mixed tray transitioned to queue tab' },
      'workflow-selected-node': { label: 'Workflow node selection changed' },
      'workflow-switched': { label: 'Adjacent workflow selected' },
      'completed-workflow-history': {
        label: 'Completed workflow retained while tray is expanded',
      },
      'expanded-via-toggle': {
        label: 'Collapsed tray expanded through the production shortcut',
      },
      'collapsed-after-toggle': {
        label: 'Expanded tray collapsed through the production shortcut',
      },
      'queue-remove-transition': {
        label: 'Queue row removed through prompt-owned delete handling',
        gapType: 'integration-only',
        description:
          'Requires the composite prompt and tray surface, not the isolated tray component.',
      },
      'queue-edit-submit-transition': {
        label: 'Queue edit opened and submitted through the prompt',
        gapType: 'integration-only',
        description:
          'Requires the composite prompt and tray surface, not the isolated tray component.',
      },
    },
    storyOrder: [
      'CollapsedWorkflows',
      'CollapsedTasksRemaining',
      'CollapsedTasksDone',
      'CollapsedQueue',
      'CollapsedMixed',
      'ExpandedTasksMixed',
      'ExpandedTasksScrolled',
      'ExpandedQueue',
      'ExpandedQueueEditing',
      'ExpandedQueueScrolled',
      'ExpandedSteerOnly',
      'ExpandedQueueAndSteer',
      'MixedTabsJourney',
      'WorkflowNavigationJourney',
      'ExpandedCompletedWorkflow',
      'ExpandCollapseJourney',
    ],
  },
};

export default meta;

export const CollapsedWorkflows = {
  args: { scenario: 'collapsed-workflows' satisfies ActivityTrayScenario },
  parameters: certification(
    'release-hardening',
    {
      visible: [
        'release-hardening running 1/2',
        'security-review paused 1/2',
        'smoke-matrix running 1/2',
        '+2 others',
        'ctrl+x expand',
      ],
      hidden: ['docs-check', 'packaging'],
    },
    ['collapsed-workflow-overflow']
  ),
};

export const CollapsedTasksRemaining = {
  args: {
    scenario: 'collapsed-tasks-remaining' satisfies ActivityTrayScenario,
  },
  parameters: certification(
    '2 tasks remaining',
    { visible: ['2 tasks remaining', 'ctrl+x expand'] },
    ['collapsed-tasks-remaining']
  ),
};

export const CollapsedTasksDone = {
  args: { scenario: 'collapsed-tasks-done' satisfies ActivityTrayScenario },
  parameters: certification(
    '2 tasks done',
    { visible: ['2 tasks done'], hidden: ['remaining'] },
    ['collapsed-tasks-complete']
  ),
};

export const CollapsedQueue = {
  args: { scenario: 'collapsed-queue' satisfies ActivityTrayScenario },
  parameters: certification(
    '3 messages queued',
    { visible: ['3 messages queued', 'ctrl+x expand'] },
    ['collapsed-queue']
  ),
};

export const CollapsedMixed = {
  args: { scenario: 'collapsed-mixed' satisfies ActivityTrayScenario },
  parameters: certification(
    'release-hardening',
    {
      visible: [
        'release-hardening running 1/2',
        '2 messages queued',
        '2 tasks remaining',
      ],
    },
    ['collapsed-mixed']
  ),
};

export const ExpandedTasksMixed = {
  args: { scenario: 'expanded-tasks-mixed' satisfies ActivityTrayScenario },
  parameters: certification(
    'Inspect the release queue',
    {
      visible: [
        'Tasks (3)',
        'Inspect the release queue',
        'Validate Windows packaging',
        'Publish verification evidence',
      ],
    },
    ['tasks-mixed']
  ),
};

export const ExpandedTasksScrolled = {
  args: {
    scenario: 'expanded-tasks-scrolled' satisfies ActivityTrayScenario,
  },
  parameters: certification(
    'Release task 5',
    {
      visible: ['Tasks (9)', 'Release task 5', 'Release task 9'],
      hidden: ['Release task 1', 'Release task 3'],
    },
    ['tasks-scrolled']
  ),
};

export const ExpandedQueue = {
  args: { scenario: 'expanded-queue' satisfies ActivityTrayScenario },
  parameters: certification(
    'Verify the Windows artifact',
    {
      visible: [
        'Messages (3)',
        '> 2. Verify the Windows artifact',
        'enter edit',
        'del remove',
      ],
    },
    ['queue-selected']
  ),
};

export const ExpandedQueueEditing = {
  args: {
    scenario: 'expanded-queue-editing' satisfies ActivityTrayScenario,
  },
  parameters: certification(
    'Verify the Windows artifact',
    {
      visible: [
        'Messages (3)',
        '2. Verify the Windows artifact',
        'esc to cancel',
      ],
      hidden: ['enter edit', 'del remove'],
    },
    ['queue-editing']
  ),
};

export const ExpandedQueueScrolled = {
  args: {
    scenario: 'expanded-queue-scrolled' satisfies ActivityTrayScenario,
  },
  parameters: certification(
    'Queued release message 8',
    {
      visible: [
        'Messages (9)',
        'Queued release message 4',
        '> 8. Queued release message 8',
      ],
      hidden: ['Queued release message 1', 'Queued release message 3'],
    },
    ['queue-scrolled']
  ),
};

export const ExpandedSteerOnly = {
  args: { scenario: 'expanded-steer-only' satisfies ActivityTrayScenario },
  parameters: certification(
    'Redirect the active turn',
    {
      visible: [
        'Messages (1)',
        'Redirect the active turn to release validation',
        'del remove',
      ],
      hidden: ['enter edit'],
    },
    ['steer-only']
  ),
};

export const ExpandedQueueAndSteer = {
  args: {
    scenario: 'expanded-queue-and-steer' satisfies ActivityTrayScenario,
  },
  parameters: certification(
    'Prioritize the active release validation',
    {
      visible: [
        'Messages (3)',
        'Prioritize the active release validation',
        '1. Review the release report',
        '> 2. Verify the Windows artifact',
      ],
    },
    ['queue-and-steer']
  ),
};

export const MixedTabsJourney = {
  args: { scenario: 'expanded-mixed-tabs' satisfies ActivityTrayScenario },
  parameters: certification(
    'release-hardening',
    {
      visible: ['Tasks (3)', 'Messages (3)', 'release-hardening (1/2)'],
    },
    [],
    {
      workflow: {
        label: 'mixed tray workflow tab',
        coversVisualStates: ['mixed-tab-workflow'],
        assertions: { visible: ['release-hardening', 'visual-check'] },
      },
      tasks: {
        label: 'mixed tray tasks tab',
        coversVisualStates: ['mixed-tab-tasks'],
        assertions: { visible: ['Validate Windows packaging'] },
      },
      queue: {
        label: 'mixed tray queue tab',
        coversVisualStates: ['mixed-tab-queue'],
        assertions: { visible: ['Review the release report'] },
      },
    }
  ),
  play: captureMixedTabs,
};

export const WorkflowNavigationJourney = {
  args: { scenario: 'expanded-workflow' satisfies ActivityTrayScenario },
  parameters: certification(
    'release-hardening',
    {
      visible: [
        'release-hardening',
        'left/right workflows',
        'select',
        'ctrl+g monitor',
      ],
    },
    [],
    {
      'first-workflow': {
        label: 'first workflow selected',
        assertions: {
          visible: ['◐ release-hardening (1/2)', '❯ ├── ✓ contract-check'],
        },
      },
      'selected-node': {
        label: 'next workflow node selected',
        coversVisualStates: ['workflow-selected-node'],
        assertions: { visible: ['❯ └── ◐ visual-check'] },
      },
      'next-workflow': {
        label: 'adjacent workflow selected',
        coversVisualStates: ['workflow-switched'],
        assertions: {
          visible: [
            '◐ security-review (1/2)',
            '❯ ├── ✓ contract-check',
            '└── ⏸ visual-check',
          ],
        },
      },
    }
  ),
  play: captureWorkflowNavigation,
};

export const ExpandedCompletedWorkflow = {
  args: {
    scenario: 'expanded-completed-workflow' satisfies ActivityTrayScenario,
  },
  parameters: certification(
    'release-certified',
    {
      visible: ['release-certified (2/2)', 'contract-check', 'visual-check'],
      hidden: ['left/right workflows'],
    },
    ['completed-workflow-history']
  ),
};

export const ExpandCollapseJourney = {
  args: { scenario: 'collapsed-mixed' satisfies ActivityTrayScenario },
  parameters: certification(
    'release-hardening',
    { visible: ['release-hardening'] },
    [],
    {
      collapsed: {
        label: 'collapsed mixed tray before shortcut',
        assertions: {
          visible: ['2 messages queued', '2 tasks remaining', 'ctrl+x expand'],
        },
      },
      expanded: {
        label: 'expanded mixed tray after shortcut',
        coversVisualStates: ['expanded-via-toggle'],
        assertions: {
          visible: ['Tasks (3)', 'Messages (2)', 'release-hardening (1/2)'],
        },
      },
      'collapsed-again': {
        label: 'collapsed mixed tray after second shortcut',
        coversVisualStates: ['collapsed-after-toggle'],
        assertions: {
          visible: ['2 messages queued', '2 tasks remaining', 'ctrl+x expand'],
        },
      },
    }
  ),
  play: captureExpandCollapse,
};
