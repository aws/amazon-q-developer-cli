import React from 'react';
import { Kiro } from '../../../kiro.js';
import { AppStoreContext, createAppStore } from '../../../stores/app-store.js';
import { workflowStore } from '../../../stores/workflow-store.js';
import type { WorkflowRunSummary } from '../../../types/workflow-history.js';
import type {
  StorybookParameters,
  StorybookPlay,
} from '../../../storybook/contracts.js';
import { WorkflowHistoryPanel } from './WorkflowHistoryPanel.js';

function run(
  workflowId: string,
  name: string,
  status: WorkflowRunSummary['status'],
  overrides: Partial<WorkflowRunSummary> = {}
): WorkflowRunSummary {
  return {
    workflowId,
    name,
    status,
    createdAt: '2026-07-20T10:00:00.000Z',
    updatedAt: '2026-07-20T10:01:00.000Z',
    parentSessionId: 'storybook-parent-session',
    ...overrides,
  };
}

type WorkflowHistoryScenario = 'running' | 'paused' | 'failed';

function seedHistory(scenario: WorkflowHistoryScenario): void {
  const byScenario: Record<WorkflowHistoryScenario, WorkflowRunSummary> = {
    running: run('visual-running', 'release-hardening', 'running'),
    paused: run('visual-paused', 'security-review', 'paused'),
    failed: run('visual-failed', 'topology-validation', 'failed'),
  };
  const others = Object.entries(byScenario)
    .filter(([key]) => key !== scenario)
    .map(([, value]) => value);
  workflowStore.getState().openWorkflowHistory([
    byScenario[scenario],
    run('visual-completed', 'smoke-baseline', 'completed', {
      startedAt: '2026-07-20T09:50:00.000Z',
      endedAt: '2026-07-20T09:55:00.000Z',
    }),
    ...others,
  ]);
}

function WorkflowHistoryPanelStory({
  scenario,
}: {
  scenario: WorkflowHistoryScenario;
}): React.ReactElement {
  const [appStore] = React.useState(() =>
    createAppStore({ kiro: new Kiro(), agentEngine: 'kas' })
  );
  React.useEffect(() => {
    seedHistory(scenario);
  }, [scenario]);
  return (
    <AppStoreContext.Provider value={appStore}>
      <WorkflowHistoryPanel onClose={() => {}} />
    </AppStoreContext.Provider>
  );
}

function certification(
  assertions: NonNullable<
    NonNullable<StorybookParameters['certification']>['assertions']
  >
): StorybookParameters {
  return {
    layout: 'fullscreen',
    capturesKeyboard: true,
    certification: {
      suite: 'workflow-monitor',
      readyText: 'WORKFLOWS',
      assertions: {
        visible: ['this session', ...(assertions.visible ?? [])],
        hidden: assertions.hidden ?? [],
      },
    },
  };
}

const meta = {
  title: 'Workflows/WorkflowHistoryPanel',
  component: WorkflowHistoryPanelStory,
  parameters: {
    layout: 'fullscreen',
    storyOrder: ['Running', 'Paused', 'Failed', 'CancelConfirmation'],
  },
};

export default meta;

export const Running = {
  args: { scenario: 'running' satisfies WorkflowHistoryScenario },
  parameters: certification({
    visible: [
      'release-hardening',
      'p pause',
      'x cancel',
      'navigate',
      'enter view',
    ],
  }),
};

export const Paused = {
  args: { scenario: 'paused' satisfies WorkflowHistoryScenario },
  parameters: certification({
    visible: ['security-review', 'r resume', 'x cancel'],
  }),
};

export const Failed = {
  args: { scenario: 'failed' satisfies WorkflowHistoryScenario },
  parameters: certification({
    visible: ['topology-validation', 'r retry'],
  }),
};

const armCancelConfirmation: StorybookPlay = async ({ type, waitFor }) => {
  await type('x');
  await waitFor('x confirm cancel');
};

export const CancelConfirmation = {
  args: { scenario: 'running' satisfies WorkflowHistoryScenario },
  parameters: certification({
    visible: ['x confirm cancel', 'esc keep running'],
  }),
  play: armCancelConfirmation,
};
