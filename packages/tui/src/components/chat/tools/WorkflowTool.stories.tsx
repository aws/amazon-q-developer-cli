import { WorkflowTool } from './WorkflowTool.js';

const workflow = JSON.stringify({
  workflow: {
    name: 'release-hardening',
    steps: [
      { type: 'step', name: 'build' },
      {
        type: 'parallel',
        branches: [
          { type: 'step', name: 'test' },
          { type: 'step', name: 'review' },
        ],
      },
    ],
  },
});

const certification = (
  readyText: string,
  visible: readonly string[] = [readyText]
) => ({
  layout: 'fullscreen' as const,
  certification: {
    suite: 'workflow-monitor',
    readyText,
    viewport: { columns: 100, rows: 12 },
    assertions: { visible },
  },
});

const meta = {
  title: 'Workflows/WorkflowTool',
  component: WorkflowTool,
  parameters: {
    layout: 'fullscreen',
    storyOrder: ['Starting', 'Started', 'Failed'],
  },
};

export default meta;

export const Starting = {
  args: {
    name: 'run_workflow',
    content: workflow,
  },
  parameters: certification('Starting workflow', [
    'Starting workflow',
    '"release-hardening" 3 steps',
  ]),
};

export const Started = {
  args: {
    name: 'run_workflow',
    content: workflow,
    isFinished: true,
    result: {
      status: 'success' as const,
      output: JSON.stringify({ workflowId: 'wf-release' }),
    },
  },
  parameters: certification('Started workflow', [
    'Started workflow',
    'ctrl+g monitor',
  ]),
};

export const Failed = {
  args: {
    name: 'run_workflow',
    content: workflow,
    isFinished: true,
    result: {
      status: 'error' as const,
      error: 'Workflow exceeds the maximum of 20 step nodes',
    },
  },
  parameters: certification('Workflow failed to start', [
    'Workflow failed to start',
    'maximum of 20 step nodes',
  ]),
};
