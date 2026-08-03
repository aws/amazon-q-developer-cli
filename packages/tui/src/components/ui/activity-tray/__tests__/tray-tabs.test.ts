import { describe, expect, it } from 'bun:test';
import {
  availableActivityTrayTabs,
  isWorkflowTrayActive,
  nextActivityTrayTab,
  resolveActivityTrayTab,
} from '../tray-tabs.js';

describe('activity tray workflow tabs', () => {
  it('includes only running and paused workflow states', () => {
    expect(isWorkflowTrayActive('running')).toBe(true);
    expect(isWorkflowTrayActive('paused')).toBe(true);
    expect(isWorkflowTrayActive('completed')).toBe(false);
    expect(isWorkflowTrayActive('aborted')).toBe(false);
  });

  it('keeps a stable tasks, messages, workflow order', () => {
    expect(
      availableActivityTrayTabs({
        hasTasks: true,
        hasMessages: true,
        hasWorkflow: true,
      })
    ).toEqual(['tasks', 'queue', 'workflow']);
  });

  it('cycles only through available tabs', () => {
    const tabs = availableActivityTrayTabs({
      hasTasks: false,
      hasMessages: true,
      hasWorkflow: true,
    });
    expect(nextActivityTrayTab(tabs, 'queue')).toBe('workflow');
    expect(nextActivityTrayTab(tabs, 'workflow')).toBe('queue');
    expect(nextActivityTrayTab(tabs, 'tasks')).toBe('queue');
  });

  it('honors an available request and otherwise prefers workflow', () => {
    const tabs = ['tasks', 'queue', 'workflow'] as const;

    expect(resolveActivityTrayTab(tabs, 'queue')).toBe('queue');
    expect(resolveActivityTrayTab(tabs, null)).toBe('workflow');
    expect(resolveActivityTrayTab(['tasks', 'queue'], 'workflow')).toBe(
      'tasks'
    );
    expect(resolveActivityTrayTab([], null)).toBeNull();
  });
});
