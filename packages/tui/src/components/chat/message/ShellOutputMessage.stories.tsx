import React from 'react';
import { Box } from './../../../renderer.js';
import type {
  StorybookParameters,
  StorybookPlay,
} from '../../../storybook/contracts.js';
import { certifyVisualStory } from '../../../storybook/story-certification.js';
import { Text } from '../../ui/text/Text.js';
import { ShellOutputMessage } from './ShellOutputMessage.js';

const viewport = { columns: 100, rows: 23 };
const settledOutput = ['dependency scan complete', 'lockfile verified'].join(
  '\n'
);
const longOutput = Array.from(
  { length: 20 },
  (_, index) => `phase ${String(index + 1).padStart(2, '0')}: release check`
).join('\n');
const collapsedHint = '2 lines hidden, ctrl+o to expand';

function ShellHistory({
  active,
}: {
  active: 'running' | 'complete' | 'long';
}): React.ReactElement {
  return (
    <Box flexDirection="column">
      <Text>Shell output history</Text>
      <ShellOutputMessage content={settledOutput} isStatic status="success" />
      {active === 'running' && (
        <ShellOutputMessage content="" isRunning status="thinking" />
      )}
      {active === 'complete' && (
        <ShellOutputMessage
          content={'tests passed\nartifacts signed\nrelease complete'}
          isStatic
          status="success"
        />
      )}
      {active === 'long' && (
        <ShellOutputMessage content={longOutput} status="active" />
      )}
    </Box>
  );
}

function journeyCertification(
  captures: NonNullable<
    NonNullable<StorybookParameters['certification']>['captures']
  >
): StorybookParameters {
  return {
    layout: 'fullscreen',
    capturesKeyboard: true,
    certification: {
      suite: 'visual-stories',
      readyText: collapsedHint,
      viewport,
      assertions: {
        hidden: ['undefined'],
      },
      captures,
    },
  };
}

const captureExpandCollapse: StorybookPlay = async ({
  type,
  waitFor,
  capture,
}) => {
  await capture('collapsed');
  await type('\x0f', { delayMs: 0 });
  await waitFor('phase 08: release check');
  await capture('expanded');
  await type('\x0f', { delayMs: 0 });
  await waitFor(collapsedHint);
  await capture('collapsed-again');
};

const meta = {
  title: 'Chat/Message/ShellOutputMessage',
  component: ShellOutputMessage,
  parameters: {
    layout: 'fullscreen',
    experience: 'tui',
    visualStates: {
      running: { label: 'Running command before output arrives' },
      complete: { label: 'Settled command output in history' },
      'long-collapsed': { label: 'Long output with middle lines collapsed' },
      'long-expanded': { label: 'Long output expanded with Ctrl+O' },
      'long-recollapsed': {
        label: 'Long output collapsed again after expansion',
      },
      'viewport-restoration-after-scroll': {
        label:
          'Collapsed output restores content scrolled out during expansion',
        gapType: 'product-limitation',
        description:
          'When expanded output scrolls the terminal, collapsing it does not restore the prior viewport position.',
      },
    },
    storyOrder: [
      'Running',
      'Complete',
      'LongCollapsed',
      'ExpandCollapseJourney',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

export const Running = {
  render: () => <ShellHistory active="running" />,
  parameters: certifyVisualStory(
    'Running...',
    {
      visible: [
        'Shell output history',
        'dependency scan complete',
        'Running...',
      ],
      ordered: ['dependency scan complete', 'Running...'],
      occurrences: { 'dependency scan complete': 1, 'Running...': 1 },
    },
    ['running'],
    viewport
  ),
};

export const Complete = {
  render: () => <ShellHistory active="complete" />,
  parameters: certifyVisualStory(
    'release complete',
    {
      visible: [
        'dependency scan complete',
        'tests passed',
        'artifacts signed',
        'release complete',
      ],
      ordered: ['dependency scan complete', 'tests passed', 'release complete'],
      hidden: ['Running...'],
      occurrences: { 'dependency scan complete': 1, 'release complete': 1 },
    },
    ['complete'],
    viewport
  ),
};

export const LongCollapsed = {
  render: () => <ShellHistory active="long" />,
  parameters: certifyVisualStory(
    collapsedHint,
    {
      visible: [
        'Shell output history',
        'dependency scan complete',
        'phase 01: release check',
        collapsedHint,
        'phase 20: release check',
      ],
      hidden: ['phase 06: release check'],
      ordered: [
        'dependency scan complete',
        'phase 01: release check',
        collapsedHint,
        'phase 20: release check',
      ],
    },
    ['long-collapsed'],
    viewport
  ),
};

export const ExpandCollapseJourney = {
  render: () => <ShellHistory active="long" />,
  parameters: journeyCertification({
    collapsed: {
      label: 'long output initially collapsed',
      coversVisualStates: ['long-collapsed'],
      assertions: {
        visible: [
          'Shell output history',
          'dependency scan complete',
          'phase 01: release check',
          collapsedHint,
          'phase 20: release check',
        ],
        hidden: ['phase 06: release check'],
        occurrences: { 'dependency scan complete': 1 },
      },
    },
    expanded: {
      label: 'long output expanded through the production Ctrl+O handler',
      coversVisualStates: ['long-expanded'],
      assertions: {
        visible: ['phase 08: release check', 'phase 20: release check'],
        hidden: [collapsedHint],
      },
    },
    'collapsed-again': {
      label: 'long output collapsed again after expansion',
      coversVisualStates: ['long-recollapsed'],
      assertions: {
        visible: [collapsedHint, 'phase 20: release check'],
        hidden: ['phase 06: release check'],
        occurrences: { 'phase 20: release check': 1 },
      },
    },
  }),
  play: captureExpandCollapse,
};
