import { describe, it, expect, mock, afterAll } from 'bun:test';
import { createAppStore, MessageRole } from './app-store';
import { AgentEventType } from '../types/agent-events';
import type { AgentStreamEvent } from '../types/agent-events';
import { Kiro } from '../kiro';

mock.module('../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    cancel: mock(),
    close: mock(),
  })),
}));

afterAll(() => {
  mock.restore();
});

function createTestStore() {
  const mockKiro = new Kiro();
  return createAppStore({ kiro: mockKiro });
}

function makeTaskToolCallEvent(
  id: string,
  command: string,
  extraArgs: Record<string, unknown> = {}
): AgentStreamEvent {
  return {
    type: AgentEventType.ToolCall,
    id,
    name: 'task',
    kind: 'other' as any,
    args: { command, ...extraArgs },
  } as AgentStreamEvent;
}

/**
 * Matches the real backend format from task_tool.rs format_full_state():
 * - task_description (not subject)
 * - completed: boolean (not status string)
 */
function makeTaskToolFinishedEvent(
  id: string,
  tasks: Array<{ id: string; task_description: string; completed: boolean }>,
  description: string = ''
): AgentStreamEvent {
  return {
    type: AgentEventType.ToolCallFinished,
    id,
    result: {
      status: 'success',
      output: JSON.stringify({ tasks, description }),
    },
  } as AgentStreamEvent;
}

describe('Task state extraction from tool events', () => {
  it('populates tasks after a task create tool call finishes', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();

    // Dispatch ToolCall (create)
    handler(
      makeTaskToolCallEvent('task-1', 'create', {
        tasks: [
          { task_description: 'Set up Express server' },
          { task_description: 'Create user endpoints' },
          { task_description: 'Add authentication' },
        ],
        task_list_description: 'Build REST API',
      })
    );

    // Verify tool message was created
    const messages = store.getState().messages;
    expect(messages).toHaveLength(1);
    expect(messages[0]!.role).toBe(MessageRole.ToolUse);

    // Verify task state is NOT yet populated (tool hasn't finished)
    expect(store.getState().tasks).toHaveLength(0);

    // Dispatch ToolCallFinished with task results (real backend format)
    handler(
      makeTaskToolFinishedEvent(
        'task-1',
        [
          {
            id: '1',
            task_description: 'Set up Express server',
            completed: false,
          },
          {
            id: '2',
            task_description: 'Create user endpoints',
            completed: false,
          },
          { id: '3', task_description: 'Add authentication', completed: false },
        ],
        'Build REST API'
      )
    );

    // NOW tasks should be populated
    const state = store.getState();
    expect(state.tasks).toHaveLength(3);
    expect(state.tasks[0]!.id).toBe('1');
    expect(state.tasks[0]!.subject).toBe('Set up Express server');
    expect(state.tasks[0]!.status).toBe('pending');
  });

  it('updates tasks after a complete command finishes', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();

    // First: create tasks
    handler(makeTaskToolCallEvent('task-1', 'create'));
    handler(
      makeTaskToolFinishedEvent('task-1', [
        { id: '1', task_description: 'Task A', completed: false },
        { id: '2', task_description: 'Task B', completed: false },
      ])
    );
    expect(store.getState().tasks).toHaveLength(2);

    // Then: complete a task
    handler(
      makeTaskToolCallEvent('task-2', 'complete', {
        completed_task_ids: ['1'],
      })
    );
    handler(
      makeTaskToolFinishedEvent('task-2', [
        { id: '1', task_description: 'Task A', completed: true },
        { id: '2', task_description: 'Task B', completed: false },
      ])
    );

    const state = store.getState();
    expect(state.tasks).toHaveLength(2);
    expect(state.tasks[0]!.status).toBe('completed');
    expect(state.tasks[1]!.status).toBe('pending');
  });

  it('does not populate tasks for non-task tool calls', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();

    // A regular tool call (not task tool)
    handler({
      type: AgentEventType.ToolCall,
      id: 'bash-1',
      name: 'execute_bash',
      kind: 'shell' as any,
      args: { command: 'echo hello' },
    } as AgentStreamEvent);

    handler({
      type: AgentEventType.ToolCallFinished,
      id: 'bash-1',
      result: { status: 'success', output: 'hello' },
    } as AgentStreamEvent);

    expect(store.getState().tasks).toHaveLength(0);
  });

  it('handles failed task tool call without crashing', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();

    handler(makeTaskToolCallEvent('task-1', 'create'));
    handler({
      type: AgentEventType.ToolCallFinished,
      id: 'task-1',
      result: { status: 'error', error: 'Something went wrong' },
    } as AgentStreamEvent);

    // Should not crash, tasks remain empty
    expect(store.getState().tasks).toHaveLength(0);
  });

  it('clears tasks when all are completed and a new create arrives', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();

    // Create with one already completed
    handler(makeTaskToolCallEvent('task-1', 'create'));
    handler(
      makeTaskToolFinishedEvent('task-1', [
        { id: '1', task_description: 'Task A', completed: true },
      ])
    );
    expect(store.getState().tasks).toHaveLength(1);

    // New create replaces
    handler(makeTaskToolCallEvent('task-2', 'create'));
    handler(
      makeTaskToolFinishedEvent('task-2', [
        { id: '2', task_description: 'Task B', completed: false },
        { id: '3', task_description: 'Task C', completed: false },
      ])
    );
    expect(store.getState().tasks).toHaveLength(2);
    expect(store.getState().tasks[0]!.id).toBe('2');
  });

  it('toggleActivityTray flips the expanded state', () => {
    const store = createTestStore();

    expect(store.getState().activityTrayExpanded).toBe(false);
    store.getState().toggleActivityTray();
    expect(store.getState().activityTrayExpanded).toBe(true);
    store.getState().toggleActivityTray();
    expect(store.getState().activityTrayExpanded).toBe(false);
  });

  it('populates tasks when output is wrapped in ToolExecutionOutput envelope (real ACP behavior)', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();

    handler(makeTaskToolCallEvent('task-1', 'create'));

    // Real ACP sends rawOutput as serde_json::Value of ToolExecutionOutput:
    // { items: [{ Json: { tasks: [...], description: "..." } }] }
    handler({
      type: AgentEventType.ToolCallFinished,
      id: 'task-1',
      result: {
        status: 'success',
        output: {
          items: [
            {
              Json: {
                tasks: [
                  { id: '1', task_description: 'Task A', completed: false },
                  { id: '2', task_description: 'Task B', completed: true },
                ],
                description: 'Test plan',
              },
            },
          ],
        },
      },
    } as AgentStreamEvent);

    const state = store.getState();
    expect(state.tasks).toHaveLength(2);
    expect(state.tasks[0]!.subject).toBe('Task A');
    expect(state.tasks[0]!.status).toBe('pending');
    expect(state.tasks[1]!.status).toBe('completed');
  });

  it('verifies tool message content contains stringified args with command field', () => {
    const store = createTestStore();
    const handler = store.getState().createStreamEventHandler();

    handler(
      makeTaskToolCallEvent('task-1', 'create', {
        tasks: [{ task_description: 'Do something' }],
      })
    );

    const msg = store.getState().messages[0]!;
    expect(msg.role).toBe(MessageRole.ToolUse);

    // The content should be JSON.stringify of the args
    const parsed = JSON.parse((msg as any).content);
    expect(parsed.command).toBe('create');
  });
});
