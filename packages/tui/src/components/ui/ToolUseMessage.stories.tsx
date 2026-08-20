import { ToolUseMessage } from './ToolUseMessage.js';
import { ToolUseStatus } from '../../stores/app-store.js';
import { certifyVisualStory } from '../../storybook/story-certification.js';

const viewport = { columns: 110, rows: 14 };

function taskStory(
  content: Record<string, unknown>,
  expected: string,
  state: string
) {
  return {
    args: {
      id: `task-${String(content.command)}`,
      name: 'task',
      content: JSON.stringify(content),
      isFinished: true,
      result: { status: 'success' as const, output: '' },
    },
    parameters: certifyVisualStory(
      expected,
      {
        visible: [expected],
        hidden: ['undefined'],
      },
      [state],
      viewport
    ),
  };
}

const meta = {
  title: 'Tools/ToolUseMessage',
  component: ToolUseMessage,
  parameters: {
    layout: 'fullscreen',
    visualStates: {
      'goal-complete': { label: 'Goal completion' },
      'goal-status': {
        label: 'Goal status',
        gapType: 'product-limitation',
        description:
          'The published schema advertises status, but the Rust goal command cannot deserialize it.',
      },
      'goal-detail': { label: 'Goal summary metadata' },
      'task-create': { label: 'Task list creation' },
      'task-complete': { label: 'Task update completion' },
      'task-add': { label: 'Tasks added' },
      'task-remove': { label: 'Tasks removed' },
      'task-list': { label: 'Tasks listed' },
      'task-output': { label: 'Rollout task result body' },
      knowledge: { label: 'Knowledge command and arguments' },
      'knowledge-output': { label: 'Rollout knowledge result body' },
      cancelled: { label: 'Cancelled tool call' },
      rejected: { label: 'Rejected tool call' },
      'question-cancelled': { label: 'Cancelled question tool call' },
      'fallback-error': { label: 'Shared fallback error renderer' },
      approval: {
        label: 'Tool waiting for permission',
        gapType: 'integration-only',
        description:
          'Approval state depends on layout key hints and permission input.',
      },
      denial: {
        label: 'Tool blocked by a permission policy',
        gapType: 'integration-only',
        description: 'Denial details require the permission-policy contract.',
      },
      'task-output-filtered': {
        label: 'Task output hidden by display policy',
        gapType: 'integration-only',
        description: 'The display-policy provider owns output visibility.',
      },
      'knowledge-output-filtered': {
        label: 'Knowledge output hidden by display policy',
        gapType: 'integration-only',
        description: 'The display-policy provider owns output visibility.',
      },
    },
    storyOrder: [
      'GoalComplete',
      'TaskCreated',
      'TasksCompleted',
      'TasksAdded',
      'TasksRemoved',
      'TasksListed',
      'TaskOutput',
      'Knowledge',
      'KnowledgeOutput',
      'Cancelled',
      'Rejected',
      'QuestionCancelled',
      'FallbackError',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

export const GoalComplete = {
  args: {
    id: 'goal-complete',
    name: 'goal',
    content: JSON.stringify({
      command: 'complete',
      summary: 'Release candidate passed certification',
    }),
    isFinished: true,
    result: { status: 'success', output: '' },
  },
  parameters: certifyVisualStory(
    'passed certification',
    {
      visible: ['Goal complete', 'Release candidate passed certification'],
      ordered: ['Goal complete', 'Release candidate passed certification'],
    },
    ['goal-complete', 'goal-detail'],
    viewport
  ),
};

export const TaskCreated = taskStory(
  {
    command: 'create',
    tasks: [
      {
        task_description: 'Build release assets',
        details: 'Build all platform binaries',
      },
    ],
    task_list_description: 'Release certification',
  },
  'Task list created',
  'task-create'
);

export const TasksCompleted = taskStory(
  {
    command: 'complete',
    completed_task_ids: ['1'],
    context_update: 'Release assets built successfully',
    modified_files: ['dist/manifest.json'],
  },
  'Tasks updated',
  'task-complete'
);

export const TasksAdded = taskStory(
  {
    command: 'add',
    new_tasks: [{ task_description: 'Verify package signatures' }],
    new_description: 'Release certification and signing',
  },
  'Tasks added',
  'task-add'
);

export const TasksRemoved = taskStory(
  {
    command: 'remove',
    remove_task_ids: ['obsolete-check'],
    new_description: 'Release certification',
  },
  'Tasks removed',
  'task-remove'
);

export const TasksListed = taskStory(
  { command: 'list' },
  'Tasks listed',
  'task-list'
);

export const TaskOutput = {
  args: {
    id: 'task-output',
    name: 'task',
    content: JSON.stringify({ command: 'list' }),
    isFinished: true,
    result: {
      status: 'success',
      output: { items: [{ Text: '1. Build complete\n2. Tests running' }] },
    },
  },
  parameters: certifyVisualStory(
    'Tests running',
    {
      visible: [
        'Tasks listed',
        'output:',
        '1. Build complete',
        '2. Tests running',
      ],
      ordered: [
        'Tasks listed',
        'output:',
        '1. Build complete',
        '2. Tests running',
      ],
    },
    ['task-list', 'task-output'],
    viewport,
    { KIRO_LITE_ROLLOUT_ENABLED: '1' }
  ),
};

export const Knowledge = {
  args: {
    id: 'knowledge-search',
    name: 'knowledge',
    content: JSON.stringify({
      command: 'search',
      query: 'workflow lifecycle',
      max_results: 5,
    }),
    isFinished: true,
    result: { status: 'success', output: '' },
  },
  parameters: certifyVisualStory(
    'max_results=5',
    {
      visible: [
        'Knowledge search',
        'query=workflow lifecycle',
        'max_results=5',
      ],
      ordered: [
        'Knowledge search',
        'query=workflow lifecycle',
        'max_results=5',
      ],
      hidden: ['output:'],
    },
    ['knowledge'],
    viewport
  ),
};

export const KnowledgeOutput = {
  args: {
    id: 'knowledge-output',
    name: 'knowledge',
    content: JSON.stringify({
      command: 'search',
      query: 'workflow lifecycle',
    }),
    isFinished: true,
    result: {
      status: 'success',
      output: { items: [{ Text: 'KNOWLEDGE_RESULT' }] },
    },
  },
  parameters: certifyVisualStory(
    'KNOWLEDGE_RESULT',
    {
      visible: [
        'Knowledge search',
        'query=workflow lifecycle',
        'output:',
        'KNOWLEDGE_RESULT',
      ],
      ordered: ['Knowledge search', 'output:', 'KNOWLEDGE_RESULT'],
    },
    ['knowledge', 'knowledge-output'],
    viewport,
    { KIRO_LITE_ROLLOUT_ENABLED: '1' }
  ),
};

export const Cancelled = {
  args: {
    id: 'read-cancelled',
    name: 'fs_read',
    content: JSON.stringify({ path: 'src/app.ts' }),
    isFinished: true,
    result: { status: 'cancelled' },
  },
  parameters: certifyVisualStory(
    'Cancelled',
    {
      visible: ['Cancelled src/app.ts'],
      hidden: ['Read src/app.ts'],
    },
    ['cancelled'],
    viewport
  ),
};

export const Rejected = {
  args: {
    id: 'write-rejected',
    name: 'fs_write',
    content: JSON.stringify({ path: 'src/config.ts' }),
    isFinished: true,
    status: ToolUseStatus.Rejected,
  },
  parameters: certifyVisualStory(
    'Rejected',
    {
      visible: ['Rejected src/config.ts'],
      hidden: ['Write src/config.ts'],
    },
    ['rejected'],
    viewport
  ),
};

export const QuestionCancelled = {
  args: {
    id: 'question-cancelled',
    name: 'Should the release continue?',
    isQuestion: true,
    content: JSON.stringify({}),
    isFinished: true,
    result: { status: 'cancelled' },
  },
  parameters: certifyVisualStory(
    'Cancelled',
    {
      visible: ['Should the release continue?', 'Cancelled'],
      ordered: ['Should the release continue?', 'Cancelled'],
    },
    ['question-cancelled'],
    viewport
  ),
};

export const FallbackError = {
  args: {
    id: 'read-error',
    name: 'fs_read',
    content: JSON.stringify({ path: 'src/missing.ts' }),
    isFinished: true,
    result: {
      status: 'error',
      error: 'File not found: src/missing.ts',
    },
  },
  parameters: certifyVisualStory(
    'File not found',
    {
      visible: ['Read src/missing.ts', 'File not found: src/missing.ts'],
      ordered: ['Read src/missing.ts', 'File not found: src/missing.ts'],
    },
    ['fallback-error'],
    viewport
  ),
};
