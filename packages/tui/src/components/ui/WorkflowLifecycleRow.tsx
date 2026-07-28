import React from 'react';
import { Box } from '../../renderer.js';
import type { StatusType } from '../../types/componentTypes.js';
import type { WorkflowLifecycleStatus } from '../../types/workflow-lifecycle.js';
import { StatusBar } from '../chat/status-bar/StatusBar.js';
import { StatusInfo } from './status/StatusInfo.js';

export interface WorkflowLifecycleRowProps {
  workflowName: string;
  status: WorkflowLifecycleStatus;
}

interface LifecyclePresentation {
  label: string;
  tone: StatusType;
}

const LIFECYCLE_PRESENTATION = {
  started: {
    label: 'started',
    tone: 'info',
  },
  completed: {
    label: 'completed',
    tone: 'success',
  },
  failed: {
    label: 'failed',
    tone: 'error',
  },
  aborted: {
    label: 'aborted',
    tone: 'warning',
  },
} satisfies Record<WorkflowLifecycleStatus, LifecyclePresentation>;

export const WorkflowLifecycleRow = React.memo(function WorkflowLifecycleRow({
  workflowName,
  status,
}: WorkflowLifecycleRowProps) {
  const presentation = LIFECYCLE_PRESENTATION[status];

  return (
    <Box marginY={1}>
      <StatusBar status={presentation.tone}>
        <StatusInfo
          title={`Workflow ${presentation.label}`}
          target={`"${workflowName}"`}
          useStatusColor={true}
          bold={true}
        />
      </StatusBar>
    </Box>
  );
});
