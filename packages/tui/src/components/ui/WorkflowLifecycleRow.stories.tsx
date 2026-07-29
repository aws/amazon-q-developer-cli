import type { WorkflowLifecycleStatus } from '../../types/workflow-lifecycle.js';
import { WorkflowLifecycleRow } from './WorkflowLifecycleRow.js';

const LIFECYCLE_TEXT: Record<WorkflowLifecycleStatus, string> = {
  started: 'Workflow started',
  completed: 'Workflow completed',
  failed: 'Workflow failed',
  aborted: 'Workflow aborted',
};

const certification = (status: WorkflowLifecycleStatus) => ({
  layout: 'fullscreen' as const,
  certification: {
    suite: 'workflow-monitor',
    readyText: LIFECYCLE_TEXT[status],
    viewport: { columns: 100, rows: 12 },
    assertions: {
      visible: [LIFECYCLE_TEXT[status], '"release-hardening"'],
      hidden: ['── WORKFLOW'],
    },
  },
});

const meta = {
  title: 'Workflows/WorkflowLifecycleRow',
  component: WorkflowLifecycleRow,
  parameters: {
    layout: 'fullscreen',
    storyOrder: ['Started', 'Completed', 'Failed', 'Aborted'],
  },
};

export default meta;

export const Started = {
  args: { workflowName: 'release-hardening', status: 'started' as const },
  parameters: certification('started'),
};

export const Completed = {
  args: { workflowName: 'release-hardening', status: 'completed' as const },
  parameters: certification('completed'),
};

export const Failed = {
  args: { workflowName: 'release-hardening', status: 'failed' as const },
  parameters: certification('failed'),
};

export const Aborted = {
  args: { workflowName: 'release-hardening', status: 'aborted' as const },
  parameters: certification('aborted'),
};
