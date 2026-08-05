import { describe, expect, it } from 'bun:test';
import {
  allocateLiteActivityRows,
  formatLiteActivitySummary,
} from '../activity-summary.js';

const SEPARATOR = ' · ';
const QUEUE_EDIT_HINT = '↑ to edit';

describe('formatLiteActivitySummary', () => {
  it('shows all actionable Lite activity when space allows', () => {
    expect(
      formatLiteActivitySummary(
        {
          runningWorkflows: 3,
          pausedWorkflows: 0,
          completedSteps: 3,
          totalSteps: 10,
          queuedMessages: 1,
          remainingTasks: 2,
          completedTasks: 1,
        },
        120,
        SEPARATOR,
        QUEUE_EDIT_HINT
      )
    ).toBe(
      '3 workflows running · steps 3/10 · 1 message queued · ↑ to edit · 2 tasks remaining · ctrl+x expand'
    );
  });

  it('compacts secondary counts before dropping the expand control', () => {
    const counts = {
      runningWorkflows: 3,
      pausedWorkflows: 0,
      completedSteps: 3,
      totalSteps: 10,
      queuedMessages: 1,
      remainingTasks: 2,
      completedTasks: 1,
    };

    expect(
      formatLiteActivitySummary(counts, 85, SEPARATOR, QUEUE_EDIT_HINT)
    ).toBe(
      '3 workflows running · steps 3/10 · 1 queued · 2 tasks · ctrl+x expand'
    );
    expect(
      formatLiteActivitySummary(counts, 50, SEPARATOR, QUEUE_EDIT_HINT)
    ).toBe('3 workflows running · steps 3/10 · ctrl+x expand');
    expect(
      formatLiteActivitySummary(counts, 20, SEPARATOR, QUEUE_EDIT_HINT)
    ).toBe('steps 3/10 · ctrl+x');
  });

  it('counts paused workflows as monitorable activity', () => {
    expect(
      formatLiteActivitySummary(
        {
          runningWorkflows: 1,
          pausedWorkflows: 2,
          completedSteps: 4,
          totalSteps: 12,
          queuedMessages: 0,
          remainingTasks: 0,
          completedTasks: 0,
        },
        80,
        SEPARATOR,
        QUEUE_EDIT_HINT
      )
    ).toBe('1 workflow running, 2 paused · steps 4/12 · ctrl+x expand');
  });

  it('shows queue and task activity without a workflow', () => {
    expect(
      formatLiteActivitySummary(
        {
          runningWorkflows: 0,
          pausedWorkflows: 0,
          completedSteps: 0,
          totalSteps: 0,
          queuedMessages: 2,
          remainingTasks: 1,
          completedTasks: 0,
        },
        80,
        SEPARATOR,
        QUEUE_EDIT_HINT
      )
    ).toBe('2 messages queued · ↑ to edit · 1 task remaining · ctrl+x expand');
  });

  it('keeps completed tasks expandable when no work remains', () => {
    expect(
      formatLiteActivitySummary(
        {
          runningWorkflows: 0,
          pausedWorkflows: 0,
          completedSteps: 0,
          totalSteps: 0,
          queuedMessages: 0,
          remainingTasks: 0,
          completedTasks: 3,
        },
        80,
        SEPARATOR,
        QUEUE_EDIT_HINT
      )
    ).toBe('3 tasks done · ctrl+x expand');
  });

  it('does not advertise expansion for queue-only activity', () => {
    expect(
      formatLiteActivitySummary(
        {
          runningWorkflows: 0,
          pausedWorkflows: 0,
          completedSteps: 0,
          totalSteps: 0,
          queuedMessages: 2,
          remainingTasks: 0,
          completedTasks: 0,
        },
        80,
        SEPARATOR,
        QUEUE_EDIT_HINT
      )
    ).toBe('2 messages queued · ↑ to edit');
  });

  it('drops the queue edit hint before truncating the queue count', () => {
    const counts = {
      runningWorkflows: 0,
      pausedWorkflows: 0,
      completedSteps: 0,
      totalSteps: 0,
      queuedMessages: 2,
      remainingTasks: 0,
      completedTasks: 0,
    };

    expect(
      formatLiteActivitySummary(counts, 20, SEPARATOR, QUEUE_EDIT_HINT)
    ).toBe('2 messages queued');
    expect(
      formatLiteActivitySummary(counts, 12, SEPARATOR, QUEUE_EDIT_HINT)
    ).toBe('2 queued');

    expect(
      formatLiteActivitySummary(
        { ...counts, remainingTasks: 1 },
        48,
        SEPARATOR,
        QUEUE_EDIT_HINT
      )
    ).toBe('2 queued · 1 task · ctrl+x expand');
  });

  it('hides when no actionable activity remains', () => {
    expect(
      formatLiteActivitySummary(
        {
          runningWorkflows: 0,
          pausedWorkflows: 0,
          completedSteps: 0,
          totalSteps: 0,
          queuedMessages: 0,
          remainingTasks: 0,
          completedTasks: 0,
        },
        80,
        SEPARATOR,
        QUEUE_EDIT_HINT
      )
    ).toBeNull();
  });

  it('shares the expanded row budget across workflows and tasks', () => {
    expect(allocateLiteActivityRows(4, 5, 6)).toEqual({
      workflows: 3,
      tasks: 3,
    });
    expect(allocateLiteActivityRows(1, 8, 6)).toEqual({
      workflows: 1,
      tasks: 5,
    });
    expect(allocateLiteActivityRows(5, 1, 6)).toEqual({
      workflows: 5,
      tasks: 1,
    });
    expect(allocateLiteActivityRows(0, 8, 6)).toEqual({
      workflows: 0,
      tasks: 6,
    });
  });
});
