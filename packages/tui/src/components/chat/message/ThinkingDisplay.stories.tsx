import React from 'react';
import { Box } from './../../../renderer.js';
import { Card } from '../../ui/card/Card.js';
import {
  ThinkingDisplay,
  type ThinkingDisplayProps,
} from './ThinkingDisplay.js';

const meta = {
  component: ThinkingDisplay,
  parameters: {
    layout: 'fullscreen',
    storyOrder: ['Collapsed', 'ExpandedMode', 'Done', 'Static'],
  },
  tags: ['autodocs'],
};

export default meta;

const TEXT = [
  'Let me look at the current auth middleware to understand the existing',
  'token validation flow. The key changes will be: detect expired access',
  'tokens, check for a valid refresh token, then issue a new access token.',
].join('\n');

const Frame = ({ children }: { children: React.ReactNode }) => (
  <Card active={true}>
    <Box flexDirection="column" paddingY={1}>
      {children}
    </Box>
  </Card>
);

export const Collapsed = {
  parameters: {
    docs: {
      storyDescription:
        'Default collapsed view while reasoning streams: header only, no body. ' +
        'Hint shows "(esc to cancel · ctrl+o for details)". Ctrl+O expands.',
    },
  },
  render: (args: ThinkingDisplayProps) => (
    <Frame>
      <ThinkingDisplay {...args} />
    </Frame>
  ),
  args: {
    text: TEXT,
    mode: 'collapsed',
  } satisfies ThinkingDisplayProps,
};

export const ExpandedMode = {
  parameters: {
    docs: {
      storyDescription:
        '`expanded` mode: the full stream is always shown and Ctrl+O is a noop ' +
        'on thinking (forceExpanded).',
    },
  },
  render: (args: ThinkingDisplayProps) => (
    <Frame>
      <ThinkingDisplay {...args} />
    </Frame>
  ),
  args: {
    text: TEXT,
    mode: 'expanded',
  } satisfies ThinkingDisplayProps,
};

export const Done = {
  parameters: {
    docs: {
      storyDescription:
        'Reasoning finished (thinkingMs set): collapsed "Thought for Ns" header, ' +
        'expandable via Ctrl+O while still in the active buffer.',
    },
  },
  render: (args: ThinkingDisplayProps) => (
    <Frame>
      <ThinkingDisplay {...args} />
    </Frame>
  ),
  args: {
    text: TEXT,
    mode: 'collapsed',
    thinkingMs: 3200,
  } satisfies ThinkingDisplayProps,
};

export const Static = {
  parameters: {
    docs: {
      storyDescription:
        'Past-turn (scrollback) rendering. Expansion is frozen and the ctrl+o ' +
        'hint is dropped since pressing it no longer affects this block.',
    },
  },
  render: (args: ThinkingDisplayProps) => (
    <Frame>
      <ThinkingDisplay {...args} />
    </Frame>
  ),
  args: {
    text: TEXT,
    mode: 'collapsed',
    thinkingMs: 3200,
    isStatic: true,
  } satisfies ThinkingDisplayProps,
};
