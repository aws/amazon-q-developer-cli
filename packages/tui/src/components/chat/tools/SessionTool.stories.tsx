import { SessionTool } from './SessionTool.js';
import { certifyVisualStory } from '../../../storybook/story-certification.js';

const viewport = { columns: 110, rows: 18 };

function actionStory(
  command: string,
  payload: Record<string, unknown>,
  expectedTarget: string
) {
  return {
    args: {
      name: 'session_management',
      isFinished: true,
      content: JSON.stringify({ command, ...payload }),
      result: { status: 'success' as const, output: '' },
    },
    parameters: certifyVisualStory(
      expectedTarget,
      {
        visible: [expectedTarget],
        hidden: ['undefined'],
      },
      ['session-target'],
      viewport
    ),
  };
}

const meta = {
  component: SessionTool,
  parameters: {
    layout: 'fullscreen',
    visualStates: {
      loading: {
        label: 'Session action in-progress indicator',
        gapType: 'visual-baseline-required',
        description:
          'Loading differs only through animated status styling, which text assertions cannot prove.',
      },
      'session-target': { label: 'Session task or target' },
      'session-command-labels': {
        label: 'Backend session commands use action-specific labels',
        gapType: 'product-limitation',
        description:
          'The renderer reads action while the backend sends command, so valid payloads use generic labels.',
      },
      error: { label: 'Session action error' },
      'crew-active': { label: 'Agent crew orchestration in progress' },
      'crew-complete': { label: 'Agent crew orchestration completed' },
      'agent-count': { label: 'Agent crew count and pluralization' },
      'task-truncated': { label: 'Long crew task is truncated' },
      pipeline: {
        label: 'Rollout subagent pipeline tree',
        gapType: 'integration-only',
        description:
          'The display-policy provider owns pipeline visibility settings.',
      },
      cancelled: {
        label: 'Session action cancelled or rejected',
        gapType: 'integration-only',
        description:
          'ToolUseMessage handles cancellation before SessionTool renders.',
      },
      responses: {
        label: 'Completed subagent response digests',
        gapType: 'integration-only',
        description:
          'Digest content is collected from session conversation stores.',
      },
      'static-persistence': {
        label: 'Past crew details follow persistence settings',
        gapType: 'integration-only',
        description: 'The display-policy provider owns static persistence.',
      },
    },
    storyOrder: [
      'Spawning',
      'Spawned',
      'CheckedSession',
      'InterruptedSession',
      'InjectedContext',
      'ManagedGroup',
      'RevivedSession',
      'Error',
      'CrewActive',
      'CrewComplete',
      'LongCrewTask',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

export const Spawning = {
  args: {
    name: 'session_management',
    isFinished: false,
    content: JSON.stringify({
      command: 'spawn_session',
      agent_name: 'release-reviewer',
      task: 'Audit the release workflow',
    }),
  },
  parameters: certifyVisualStory(
    'Audit the release workflow',
    {
      visible: ['Audit the release workflow'],
      hidden: ['undefined'],
    },
    ['session-target'],
    viewport
  ),
};

export const Spawned = actionStory(
  'spawn_session',
  {
    agent_name: 'release-reviewer',
    task: 'Audit the release workflow',
  },
  'Audit the release workflow'
);

export const CheckedSession = actionStory(
  'get_session_status',
  { target: 'worker-1' },
  'worker-1'
);

export const InterruptedSession = actionStory(
  'interrupt',
  { target: 'worker-2', message: 'Prioritize release validation' },
  'worker-2'
);

export const InjectedContext = actionStory(
  'inject_context',
  { target: 'worker-3', context: 'Release branch is frozen' },
  'worker-3'
);

export const ManagedGroup = actionStory(
  'manage_group',
  { action: 'add', group: 'reviewers', target: 'worker-3' },
  'worker-3'
);

export const RevivedSession = actionStory(
  'revive_session',
  { target: 'worker-4', task: 'Retry Windows packaging' },
  'worker-4'
);

export const Error = {
  args: {
    name: 'session_management',
    isFinished: true,
    content: JSON.stringify({
      command: 'get_session_status',
      target: 'missing-worker',
    }),
    result: {
      status: 'error',
      error: 'Session missing-worker was not found',
    },
  },
  parameters: certifyVisualStory(
    'was not found',
    {
      visible: ['missing-worker', 'Session missing-worker was not found'],
      ordered: ['missing-worker', 'Session missing-worker was not found'],
    },
    ['session-target', 'error'],
    viewport
  ),
};

const crewContent = JSON.stringify({
  task: 'Certify release candidate',
  stages: [
    {
      name: 'build',
      role: 'builder',
      prompt_template: 'Build {task}',
    },
    {
      name: 'test',
      role: 'tester',
      depends_on: ['build'],
      prompt_template: 'Test {task}',
    },
    {
      name: 'review',
      role: 'reviewer',
      depends_on: ['test'],
      prompt_template: 'Review {task}',
    },
  ],
});

export const CrewActive = {
  args: {
    name: 'agent_crew',
    isFinished: false,
    content: JSON.stringify({
      task: 'Certify release candidate',
      stages: [{ name: 'build' }],
    }),
  },
  parameters: certifyVisualStory(
    '1 agent',
    {
      visible: ['Orchestrating (1 agent)'],
      hidden: ['Orchestrated', 'pipeline:'],
    },
    ['crew-active', 'agent-count'],
    viewport
  ),
};

export const CrewComplete = {
  args: {
    id: 'crew-certification',
    name: 'agent_crew',
    isFinished: true,
    content: crewContent,
    result: { status: 'success', output: 'complete' },
  },
  parameters: certifyVisualStory(
    'Orchestrated',
    {
      visible: ['Orchestrated (3 agents)'],
      hidden: ['pipeline:', '[build]', '[review]'],
    },
    ['crew-complete', 'agent-count'],
    viewport,
    { KIRO_LITE_ROLLOUT_ENABLED: '1' }
  ),
};

export const LongCrewTask = {
  args: {
    name: 'agent_crew',
    isFinished: false,
    content: JSON.stringify({
      task: 'Investigate every workflow transition across all supported terminal environments',
    }),
  },
  parameters: certifyVisualStory(
    'Investigate every workflow transition',
    {
      visible: ['Orchestrating "Investigate every workflow transition ac…"'],
      hidden: ['supported terminal environments'],
    },
    ['crew-active', 'task-truncated'],
    viewport
  ),
};
