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

function certification(
  readyText: string,
  visible: readonly string[],
  hidden: readonly string[],
  coversVisualStates: readonly string[]
) {
  return {
    coversVisualStates,
    certification: {
      suite: 'workflow-monitor',
      readyText,
      viewport: { columns: 100, rows: 12 },
      assertions: {
        visible,
        hidden: ['undefined', ...hidden],
        ordered: visible,
      },
    },
  };
}

const meta = {
  title: 'Workflows/WorkflowTool',
  component: WorkflowTool,
  parameters: {
    layout: 'fullscreen',
    visualStates: {
      'launch-starting': { label: 'Workflow launch in progress' },
      'launch-started': { label: 'Workflow launch completed' },
      'launch-failed': { label: 'Workflow launch failed' },
      'launch-cancelled': { label: 'Workflow launch cancelled' },
      'inspect-checking': { label: 'Workflow inspection in progress' },
      'inspect-checked': { label: 'Workflow inspection completed' },
      'inspect-failed': { label: 'Workflow inspection failed' },
      'inspect-cancelled': { label: 'Workflow inspection cancelled' },
      'singular-step': { label: 'Singular workflow step count' },
      'output-identity': {
        label: 'Workflow identity resolved from the result envelope',
      },
      'missing-metadata': {
        label: 'Workflow launch without identity metadata',
      },
      'monitor-navigation': {
        label: 'Ctrl+G opens the launched workflow monitor',
        gapType: 'integration-only',
        description: 'Monitor navigation is owned by the layout input host.',
      },
      'pending-approval': {
        label: 'Workflow launch awaiting permission',
        gapType: 'integration-only',
        description: 'Approval state is outer ToolUseMessage composition.',
      },
      'inspect-error-identity': {
        label: 'Failed or cancelled inspection retains workflow identity',
      },
    },
    storyOrder: [
      'Starting',
      'Started',
      'Failed',
      'LaunchCancelled',
      'InspectChecking',
      'InspectChecked',
      'InspectFailed',
      'InspectCancelled',
      'OutputFallbackSingular',
      'MissingMetadata',
    ],
  },
};

export default meta;

export const Starting = {
  args: {
    name: 'run_workflow',
    content: workflow,
  },
  parameters: certification(
    'Starting workflow',
    ['Starting workflow', '"release-hardening" 3 steps'],
    ['Started workflow'],
    ['launch-starting']
  ),
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
  parameters: certification(
    'Started workflow',
    ['Started workflow', '"release-hardening"', '3 steps', 'ctrl+g monitor'],
    ['Starting workflow', 'failed'],
    ['launch-started']
  ),
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
  parameters: certification(
    'Workflow failed to start',
    [
      'Workflow failed to start',
      '"release-hardening"',
      'Workflow exceeds the maximum of 20 step nodes',
    ],
    ['Started workflow'],
    ['launch-failed']
  ),
};

export const LaunchCancelled = {
  args: {
    name: 'run_workflow',
    content: workflow,
    isFinished: true,
    result: {
      status: 'cancelled' as const,
    },
  },
  parameters: certification(
    'Workflow start cancelled',
    ['Workflow start cancelled', '"release-hardening"'],
    ['Started workflow', 'ctrl+g monitor'],
    ['launch-cancelled']
  ),
};

export const InspectChecking = {
  args: {
    name: 'inspect_workflow',
    content: JSON.stringify({ workflowId: 'wf-release' }),
  },
  parameters: certification(
    'Checking workflow status',
    ['Checking workflow status', '"wf-release"'],
    ['Checked workflow status'],
    ['inspect-checking']
  ),
};

export const InspectChecked = {
  args: {
    name: 'inspect_workflow',
    content: JSON.stringify({ workflowId: 'wf-release' }),
    isFinished: true,
    result: {
      status: 'success' as const,
      output: JSON.stringify({ workflowId: 'wf-release', status: 'running' }),
    },
  },
  parameters: certification(
    'Checked workflow status',
    ['Checked workflow status', '"wf-release"'],
    ['Checking workflow status'],
    ['inspect-checked']
  ),
};

export const InspectFailed = {
  args: {
    name: 'inspect_workflow',
    content: JSON.stringify({ workflowId: 'wf-missing' }),
    isFinished: true,
    result: {
      status: 'error' as const,
      error: 'Workflow was not found',
    },
  },
  parameters: certification(
    'Workflow inspection failed',
    ['Workflow inspection failed', '"wf-missing"', 'Workflow was not found'],
    ['Checked workflow status'],
    ['inspect-failed', 'inspect-error-identity']
  ),
};

export const InspectCancelled = {
  args: {
    name: 'inspect_workflow',
    content: JSON.stringify({ workflowId: 'wf-release' }),
    isFinished: true,
    result: {
      status: 'cancelled' as const,
    },
  },
  parameters: certification(
    'Workflow inspection cancelled',
    ['Workflow inspection cancelled', '"wf-release"'],
    ['Checked workflow status'],
    ['inspect-cancelled']
  ),
};

export const OutputFallbackSingular = {
  args: {
    name: 'run_workflow',
    content: JSON.stringify({}),
    isFinished: true,
    result: {
      status: 'success' as const,
      output: {
        items: [
          {
            Json: {
              workflowName: 'wf-release',
              steps: [{ type: 'step', name: 'only-step' }],
            },
          },
        ],
      },
    },
  },
  parameters: certification(
    'Started workflow',
    ['Started workflow', '"wf-release"', '1 step', 'ctrl+g monitor'],
    ['1 steps'],
    ['launch-started', 'singular-step', 'output-identity']
  ),
};

export const MissingMetadata = {
  args: {
    name: 'run_workflow',
    content: JSON.stringify({}),
  },
  parameters: certification(
    'Starting workflow',
    ['Starting workflow'],
    ['undefined', 'steps', 'ctrl+g monitor'],
    ['launch-starting', 'missing-metadata']
  ),
};
