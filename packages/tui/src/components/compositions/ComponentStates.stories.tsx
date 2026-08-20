import React, { useState } from 'react';
import { Box, Text } from '../../renderer.js';
import type {
  StorybookAssertions,
  StorybookParameters,
  StorybookPlay,
} from '../../storybook/contracts.js';
import { StageRow } from '../layout/crew-monitor/StageRow.js';
import type { Stage } from '../layout/crew-monitor/types.js';
import { MessageInput } from '../multi-agent/MessageInput.js';
import { KeyHints } from '../ui/hint/KeyHints.js';
import { MessageRenderer } from '../ui/MessageRenderer.js';

const viewport = { columns: 132, rows: 42 };

const meta = {
  title: 'Compositions/ComponentStates',
  parameters: {
    layout: 'fullscreen',
    experience: 'tui',
    storyOrder: [
      'CrewStageMatrix',
      'MessageRendererMatrix',
      'KeyHintLayouts',
      'HiddenKeyHints',
      'MessageInputJourney',
    ],
    visualStates: {
      'stage-state-matrix': {
        label:
          'Crew stages in pending, executing, completed, and failed states',
      },
      'stage-selected': {
        label: 'Selected crew stage row',
      },
      'stage-dependency': {
        label: 'Crew stage dependency column',
      },
      'stage-loop': {
        label: 'Crew stage loop iteration',
      },
      'stage-approval': {
        label: 'Crew stage waiting for tool approval',
      },
      'message-user': {
        label: 'Legacy message renderer user block',
      },
      'message-assistant': {
        label: 'Legacy message renderer assistant text and code blocks',
      },
      'message-tool': {
        label: 'Legacy message renderer tool call and output blocks',
      },
      'message-streaming': {
        label: 'Legacy message renderer streaming indicator',
      },
      'key-hints-left': {
        label: 'Left-aligned keyboard hint strip',
      },
      'key-hints-right': {
        label: 'Right-aligned keyboard hint strip',
      },
      'key-hints-hidden': {
        label: 'Keyboard hint strip hidden by its visibility contract',
      },
      'message-input-empty': {
        label: 'Cross-session message input before typing',
      },
      'message-input-draft': {
        label: 'Cross-session message input with a draft',
      },
      'message-input-edited': {
        label: 'Cross-session message input after backspace editing',
      },
      'message-input-submitted': {
        label: 'Cross-session message input after submission',
      },
      'message-input-cancelled': {
        label: 'Cross-session message input after cancellation',
      },
    },
  },
};

export default meta;

type Captures = NonNullable<
  NonNullable<StorybookParameters['certification']>['captures']
>;

function certification(
  readyText: string,
  assertions: StorybookAssertions,
  coversVisualStates: readonly string[],
  captures?: Captures
): StorybookParameters {
  return {
    layout: 'fullscreen',
    experience: 'tui',
    capturesKeyboard: captures !== undefined,
    ...(captures ? {} : { coversVisualStates }),
    certification: {
      suite: 'visual-stories',
      readyText,
      viewport,
      assertions: {
        visible: assertions.visible ?? [readyText],
        hidden: ['undefined', ...(assertions.hidden ?? [])],
        ordered: assertions.ordered,
        occurrences: assertions.occurrences,
      },
      ...(captures ? { captures } : {}),
    },
  };
}

function stage(
  name: string,
  agentName: string,
  state: Stage['state'],
  activeStatus: string,
  overrides: Partial<Stage> = {}
): Stage {
  return {
    name,
    agentName,
    state,
    activeStatus,
    description: `${name} stage`,
    events: 1,
    role: 'worker',
    sessionId: `stage-${name.toLowerCase()}`,
    ...overrides,
  };
}

const stages = [
  stage('Plan', 'planner', 'Pending', 'Queued'),
  stage('Build', 'builder', 'Executing', 'Compiling assets', {
    dependsOn: ['Plan'],
  }),
  stage('Verify', 'verifier', 'Completed', 'Passed', {
    dependsOn: ['Build'],
    hasLoop: true,
    loopIteration: 2,
    loopMaxIterations: 4,
  }),
  stage('Publish', 'release-manager', 'Failed', 'Tool approval needed', {
    dependsOn: ['Verify'],
  }),
];

function CrewStageSurface(): React.ReactElement {
  return (
    <Box flexDirection="column" width={120}>
      <Text>Crew stage row contract</Text>
      {stages.map((item, index) => (
        <StageRow
          key={item.name}
          stage={item}
          index={index + 1}
          isSelected={index === 1}
          hasPendingApproval={index === 3}
          nameW={18}
          agentNameW={20}
          depLabel={item.dependsOn?.join(',') ?? ''}
          depW={8}
        />
      ))}
    </Box>
  );
}

export const CrewStageMatrix = {
  render: CrewStageSurface,
  parameters: certification(
    'Crew stage row contract',
    {
      visible: [
        'Plan',
        'planner',
        'Queued',
        'Build',
        'builder',
        'Compiling assets',
        'Verify',
        'verifier',
        'Passed',
        '[3/4]',
        'Publish',
        'release-manager',
        'Tool approval needed',
      ],
      ordered: ['Plan', 'Build', 'Verify', 'Publish'],
    },
    [
      'stage-state-matrix',
      'stage-selected',
      'stage-dependency',
      'stage-loop',
      'stage-approval',
    ]
  ),
};

function MessageRendererSurface(): React.ReactElement {
  const timestamp = new Date('2026-08-19T12:00:00.000Z');
  return (
    <Box flexDirection="column">
      <Text>Legacy message renderer contract</Text>
      <MessageRenderer
        message={{
          type: 'user',
          timestamp,
          blocks: [
            {
              type: 'text',
              content: 'MESSAGE_RENDERER_USER inspect the release.',
            },
          ],
        }}
      />
      <MessageRenderer
        message={{
          type: 'assistant',
          timestamp,
          blocks: [
            {
              type: 'text',
              content: 'MESSAGE_RENDERER_ASSISTANT applying the patch.',
            },
            {
              type: 'code',
              language: 'typescript',
              content: 'export const rendererReady = true;',
            },
          ],
        }}
      />
      <MessageRenderer
        message={{
          type: 'tool',
          timestamp,
          blocks: [
            {
              type: 'tool_call',
              content: 'MESSAGE_RENDERER_TOOL execute verification',
            },
            {
              type: 'tool_output',
              content: 'MESSAGE_RENDERER_OUTPUT passed',
            },
          ],
        }}
      />
      <MessageRenderer
        isStreaming
        message={{
          type: 'assistant',
          timestamp,
          blocks: [
            {
              type: 'text',
              content: 'MESSAGE_RENDERER_STREAMING active response',
            },
          ],
        }}
      />
    </Box>
  );
}

export const MessageRendererMatrix = {
  render: MessageRendererSurface,
  parameters: certification(
    'Legacy message renderer contract',
    {
      visible: [
        'MESSAGE_RENDERER_USER inspect the release.',
        'MESSAGE_RENDERER_ASSISTANT applying the patch.',
        'Language: typescript',
        'export const rendererReady = true;',
        'MESSAGE_RENDERER_TOOL execute verification',
        'MESSAGE_RENDERER_OUTPUT passed',
        'MESSAGE_RENDERER_STREAMING active response',
        '...',
      ],
      ordered: [
        'MESSAGE_RENDERER_USER',
        'MESSAGE_RENDERER_ASSISTANT',
        'export const rendererReady = true;',
        'MESSAGE_RENDERER_TOOL',
        'MESSAGE_RENDERER_OUTPUT',
        'MESSAGE_RENDERER_STREAMING',
        '...',
      ],
    },
    ['message-user', 'message-assistant', 'message-tool', 'message-streaming']
  ),
};

function KeyHintSurface({ visible = true }: { visible?: boolean }) {
  const hints = [
    { keys: 'up/down', label: 'navigate' },
    { keys: 'enter', label: 'select' },
    { keys: 'esc', label: 'close' },
  ];
  return (
    <Box flexDirection="column" width={90}>
      <Text>Keyboard hint contract</Text>
      <Text>Left aligned</Text>
      <KeyHints hints={hints} align="left" visible={visible} />
      <Text>Right aligned</Text>
      <KeyHints hints={hints} align="right" visible={visible} />
    </Box>
  );
}

export const KeyHintLayouts = {
  render: () => <KeyHintSurface />,
  parameters: certification(
    'Keyboard hint contract',
    {
      visible: [
        'Left aligned',
        'Right aligned',
        'up/down navigate',
        'enter select',
        'esc close',
      ],
      occurrences: {
        'up/down navigate': 2,
        'enter select': 2,
        'esc close': 2,
      },
    },
    ['key-hints-left', 'key-hints-right']
  ),
};

export const HiddenKeyHints = {
  render: () => <KeyHintSurface visible={false} />,
  parameters: certification(
    'Keyboard hint contract',
    {
      visible: ['Left aligned', 'Right aligned'],
      hidden: ['up/down navigate', 'enter select', 'esc close'],
    },
    ['key-hints-hidden']
  ),
};

function MessageInputSurface(): React.ReactElement {
  const [outcome, setOutcome] = useState('none');
  return (
    <Box flexDirection="column" width={86}>
      <Text>Message input contract</Text>
      <MessageInput
        targetSessionId="release-session"
        targetSessionName="release-verifier"
        onSend={(message) => setOutcome(`sent ${message}`)}
        onCancel={() => setOutcome('cancelled')}
      />
      <Text>{`Delivery contract: ${outcome}`}</Text>
    </Box>
  );
}

const captureMessageInput: StorybookPlay = async ({
  type,
  press,
  waitFor,
  capture,
}) => {
  await capture('empty');
  await type('release candidates');
  await waitFor('release candidates');
  await capture('draft');
  await press('backspace');
  await waitFor('release candidate');
  await capture('edited');
  await type('s');
  await press('enter');
  await waitFor('Delivery contract: sent release candidates');
  await capture('submitted');
  await type('cancel me');
  await press('escape');
  await waitFor('Delivery contract: cancelled');
  await capture('cancelled');
};

export const MessageInputJourney = {
  render: MessageInputSurface,
  parameters: certification(
    'Message input contract',
    { visible: ['Message input contract'] },
    [],
    {
      empty: {
        label: 'message input before typing',
        coversVisualStates: ['message-input-empty'],
        assertions: {
          visible: [
            'Send message to release-verifier',
            'Press Enter to send, Esc to cancel',
            'Delivery contract: none',
          ],
          hidden: ['release candidates'],
        },
      },
      draft: {
        label: 'message input with draft',
        coversVisualStates: ['message-input-draft'],
        assertions: {
          visible: ['release candidates', 'Delivery contract: none'],
        },
      },
      edited: {
        label: 'message input after backspace editing',
        coversVisualStates: ['message-input-edited'],
        assertions: {
          visible: ['release candidate', 'Delivery contract: none'],
          hidden: ['release candidates'],
        },
      },
      submitted: {
        label: 'message input after submission',
        coversVisualStates: ['message-input-submitted'],
        assertions: {
          visible: ['Delivery contract: sent release candidates'],
          occurrences: { 'release candidates': 1 },
        },
      },
      cancelled: {
        label: 'message input after cancellation',
        coversVisualStates: ['message-input-cancelled'],
        assertions: {
          visible: ['cancel me', 'Delivery contract: cancelled'],
        },
      },
    }
  ),
  play: captureMessageInput,
};
