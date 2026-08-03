import { describe, expect, it } from 'bun:test';
import {
  activityTrayHints,
  activityTrayScrollOffset,
  collapsedWorkflowSummary,
  isActivityTrayVisible,
  workflowActivityLabel,
} from '../tray-view-model.js';
import type { WorkflowRunView } from '../../../../types/workflow-monitor.js';

function workflow(
  index: number,
  status: WorkflowRunView['status']
): WorkflowRunView {
  return {
    workflowId: `workflow-${index}`,
    parentSessionId: 'parent',
    name: `workflow ${index}`,
    status,
    nodes: [
      {
        id: `done-${index}`,
        type: 'step',
        status: 'completed',
        label: 'done',
        parentId: null,
        depth: 0,
      },
      {
        id: `active-${index}`,
        type: 'step',
        status: status === 'paused' ? 'paused' : 'running',
        label: 'active',
        parentId: null,
        depth: 0,
      },
    ],
    stepSessions: [],
    startedAt: index,
    completedAt: null,
  };
}

describe('activity tray view model', () => {
  it('tracks visibility across tasks, messages, and workflow retention', () => {
    const hidden = {
      hasTasks: false,
      hasMessages: false,
      liveWorkflowCount: 0,
      workflowCount: 0,
      expanded: false,
    };

    expect(isActivityTrayVisible(hidden)).toBe(false);
    expect(isActivityTrayVisible({ ...hidden, hasTasks: true })).toBe(true);
    expect(isActivityTrayVisible({ ...hidden, hasMessages: true })).toBe(true);
    expect(isActivityTrayVisible({ ...hidden, liveWorkflowCount: 1 })).toBe(
      true
    );
    expect(isActivityTrayVisible({ ...hidden, workflowCount: 1 })).toBe(false);
    expect(
      isActivityTrayVisible({
        ...hidden,
        workflowCount: 1,
        expanded: true,
      })
    ).toBe(true);
  });

  it('summarizes concurrent workflow states', () => {
    expect(workflowActivityLabel(2, 0)).toBe('2 workflows running');
    expect(workflowActivityLabel(1, 1)).toBe('1 workflow running, 1 paused');
    expect(workflowActivityLabel(1, 0)).toBeNull();
  });

  it('shows the first three workflow statuses and progress, then rolls up the rest', () => {
    const summary = collapsedWorkflowSummary([
      workflow(1, 'running'),
      workflow(2, 'paused'),
      workflow(3, 'running'),
      workflow(4, 'paused'),
      workflow(5, 'running'),
    ]);

    expect(summary).toEqual({
      entries: [
        {
          workflowId: 'workflow-1',
          name: 'workflow 1',
          status: 'running',
          completedSteps: 1,
          totalSteps: 2,
        },
        {
          workflowId: 'workflow-2',
          name: 'workflow 2',
          status: 'paused',
          completedSteps: 1,
          totalSteps: 2,
        },
        {
          workflowId: 'workflow-3',
          name: 'workflow 3',
          status: 'running',
          completedSteps: 1,
          totalSteps: 2,
        },
      ],
      hiddenCount: 2,
    });
  });

  it('keeps the next task and selected queue item visible', () => {
    expect(
      activityTrayScrollOffset({
        activeTab: 'tasks',
        itemCount: 0,
        taskStatuses: [
          'completed',
          'completed',
          'completed',
          'completed',
          'completed',
          'pending',
          'pending',
          'pending',
        ],
        selectedIndex: 0,
        maxVisible: 6,
      })
    ).toBe(2);
    expect(
      activityTrayScrollOffset({
        activeTab: 'queue',
        itemCount: 10,
        taskStatuses: [],
        selectedIndex: 8,
        maxVisible: 6,
      })
    ).toBe(4);
  });

  it('describes only controls available on the workflow tab', () => {
    expect(
      activityTrayHints({
        activeTab: 'workflow',
        queueCount: 0,
        hasSteer: false,
        editing: false,
        workflowNodeCount: 3,
        workflowCount: 2,
        tabCount: 3,
        arrows: 'up/down',
      })
    ).toEqual([
      'shift+up/down select',
      'ctrl+g monitor',
      'shift+left/right workflows',
      'tab switch',
      'ctrl+x collapse',
    ]);
  });
});
