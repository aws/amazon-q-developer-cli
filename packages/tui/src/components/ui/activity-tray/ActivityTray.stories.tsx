import React, { useState } from 'react';
import type { StoreApi } from 'zustand';
import { Box } from '../../../renderer.js';
import { Kiro } from '../../../kiro.js';
import {
  AppStoreContext,
  createAppStore,
  type AppStoreApi,
} from '../../../stores/app-store.js';
import {
  createWorkflowStore,
  type WorkflowStoreState,
} from '../../../stores/workflow-store.js';
import type {
  WorkflowMonitorNode,
  WorkflowRunView,
} from '../../../types/workflow-monitor.js';
import type { WorkflowStatus } from '../../../types/workflow.js';
import { ActivityTrayCollapsed } from './ActivityTrayCollapsed.js';
import { ActivityTrayExpanded } from './ActivityTrayExpanded.js';

type ActivityTrayScenario = 'collapsed-multiple' | 'expanded-workflow';

interface ActivityTrayStoryProps {
  scenario: ActivityTrayScenario;
}

interface ActivityTrayStoryStores {
  appStore: AppStoreApi;
  workflowStore: StoreApi<WorkflowStoreState>;
}

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
    agentName: 'wf-coder',
  };
}

function workflowRun(
  workflowId: string,
  name: string,
  status: WorkflowStatus,
  order: number
): WorkflowRunView {
  return {
    workflowId,
    parentSessionId: 'tray-story-parent',
    name,
    status,
    nodes: [
      workflowStep(workflowId, 'contract-check', 'completed'),
      workflowStep(
        workflowId,
        'visual-check',
        status === 'paused' ? 'paused' : 'running'
      ),
    ],
    stepSessions: [],
    startedAt: Date.parse('2026-07-20T17:00:00.000Z') - order * 1000,
    completedAt: null,
  };
}

function createStoryStores(): ActivityTrayStoryStores {
  const runs = [
    workflowRun('tray-1', 'release-hardening', 'running', 1),
    workflowRun('tray-2', 'security-review', 'paused', 2),
    workflowRun('tray-3', 'smoke-matrix', 'running', 3),
    workflowRun('tray-4', 'docs-check', 'paused', 4),
    workflowRun('tray-5', 'packaging', 'running', 5),
  ];
  const workflowStore = createWorkflowStore();
  runs.forEach((run) => workflowStore.getState().openHistoricalWorkflow(run));
  workflowStore.getState().setActiveWorkflow(runs[0]!.workflowId);

  const appStore = createAppStore({
    kiro: new Kiro(),
    agentEngine: 'kas',
  });
  appStore.setState({
    queuedMessages: ['Review the final report'],
  });
  return { appStore, workflowStore };
}

function ActivityTrayStory({
  scenario,
}: ActivityTrayStoryProps): React.ReactElement {
  const [stores] = useState(createStoryStores);
  return (
    <AppStoreContext.Provider value={stores.appStore}>
      <Box flexDirection="column" width="100%" height="100%">
        {scenario === 'collapsed-multiple' ? (
          <ActivityTrayCollapsed
            hasTasks={false}
            queuedMessageCount={1}
            store={stores.workflowStore}
          />
        ) : (
          <ActivityTrayExpanded
            activeTab="workflow"
            hasTasks={false}
            inputOwnership={{
              rowNavigationActive: false,
              tabNavigationActive: true,
              workflowNavigationActive: true,
            }}
            navigationActive
            store={stores.workflowStore}
            tabs={['queue', 'workflow']}
          />
        )}
      </Box>
    </AppStoreContext.Provider>
  );
}

const meta = {
  title: 'Workflows/ActivityTray',
  component: ActivityTrayStory,
  parameters: {
    layout: 'fullscreen',
    storyOrder: ['CollapsedMultiple', 'ExpandedWorkflow'],
  },
};

export default meta;

export const CollapsedMultiple = {
  args: { scenario: 'collapsed-multiple' satisfies ActivityTrayScenario },
  parameters: {
    certification: {
      suite: 'workflow-monitor',
      readyText: 'release-hardening',
      viewport: { columns: 150, rows: 12 },
      assertions: {
        visible: [
          'release-hardening running 1/2',
          'security-review paused 1/2',
          'smoke-matrix running 1/2',
          '+2 others',
          '1 message queued',
          'ctrl+x expand',
        ],
        hidden: ['docs-check', 'packaging', 'undefined'],
      },
    },
  },
};

export const ExpandedWorkflow = {
  args: { scenario: 'expanded-workflow' satisfies ActivityTrayScenario },
  parameters: {
    certification: {
      suite: 'workflow-monitor',
      readyText: 'release-hardening',
      viewport: { columns: 120, rows: 16 },
      assertions: {
        visible: [
          'release-hardening',
          'release-hardening (1/2)',
          'left/right workflows',
          'tab switch',
          'ctrl+x collapse',
        ],
        hidden: ['1-9 jump', 'ctrl+x to collapse', 'undefined'],
      },
    },
  },
};
