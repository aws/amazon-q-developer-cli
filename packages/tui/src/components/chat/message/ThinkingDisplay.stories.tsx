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
    storyOrder: [
      'Short',
      'LongCollapsed',
      'LongExpanded',
      'Static',
      'TrailingNewlines',
      'Paragraphed',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

const SHORT_TEXT = ['Plan the approach.', 'Pick the right hook.'].join('\n');

const LONG_TEXT = [
  'Step 1: Read the existing thinking components.',
  'Step 2: Identify the shared rendering shape.',
  'Step 3: Confirm the useExpandableOutput hook contract.',
  'Step 4: Decide whether to share toolOutputsExpanded or add a new flag.',
  'Step 5: Sketch the component skeleton.',
  'Step 6: Wire ConversationView to the new component.',
  'Step 7: Remove the now-redundant ThinkingSummary file.',
  'Step 8: Add storybook coverage for the new states.',
  'Step 9: Extend the integ test for ctrl+o expansion.',
  'Step 10: Run typecheck/lint/tests.',
  'Step 11: Verify in storybook visually.',
].join('\n');

const Frame = ({ children }: { children: React.ReactNode }) => (
  <Card active={true}>
    <Box flexDirection="column" paddingY={1}>
      {children}
    </Box>
  </Card>
);

export const Short = {
  parameters: {
    docs: {
      storyDescription:
        'Short thinking that fits below PREVIEW_LINES — no expand hint.',
    },
  },
  render: (args: ThinkingDisplayProps) => (
    <Frame>
      <ThinkingDisplay {...args} />
    </Frame>
  ),
  args: {
    text: SHORT_TEXT,
  } satisfies ThinkingDisplayProps,
};

export const LongCollapsed = {
  parameters: {
    docs: {
      storyDescription:
        'Long thinking, default collapsed view: tail of 4 lines plus the ' +
        '"...+N lines above (ctrl+o to toggle)" hint. This is also what the ' +
        'live streaming view looks like — the body grows as new lines arrive ' +
        'and the hint count ticks up.',
    },
  },
  render: (args: ThinkingDisplayProps) => (
    <Frame>
      <ThinkingDisplay {...args} />
    </Frame>
  ),
  args: {
    text: LONG_TEXT,
  } satisfies ThinkingDisplayProps,
};

export const LongExpanded = {
  parameters: {
    docs: {
      storyDescription:
        'Same content, expanded view. In the running TUI this is reached via ctrl+o; ' +
        'in storybook it requires `toolOutputsExpanded=true` in the story store.',
    },
  },
  render: (args: ThinkingDisplayProps) => (
    <Frame>
      <ThinkingDisplay {...args} />
    </Frame>
  ),
  args: {
    text: LONG_TEXT,
  } satisfies ThinkingDisplayProps,
};

export const Static = {
  parameters: {
    docs: {
      storyDescription:
        'Past-turn (scrollback) rendering. Expansion state is frozen via the hook ' +
        'snapshot, so ctrl+o no longer affects this block; the elision hint drops ' +
        'the "(ctrl+o to toggle)" suffix to avoid suggesting a dead keybinding.',
    },
  },
  render: (args: ThinkingDisplayProps) => (
    <Frame>
      <ThinkingDisplay {...args} />
    </Frame>
  ),
  args: {
    text: LONG_TEXT,
    isStatic: true,
  } satisfies ThinkingDisplayProps,
};

export const TrailingNewlines = {
  parameters: {
    docs: {
      storyDescription:
        'Trailing newlines are stripped so the preview window is not stolen by blanks.',
    },
  },
  render: (args: ThinkingDisplayProps) => (
    <Frame>
      <ThinkingDisplay {...args} />
    </Frame>
  ),
  args: {
    text: SHORT_TEXT + '\n\n\n',
  } satisfies ThinkingDisplayProps,
};

const PARAGRAPHED_TEXT = [
  'I should think about this carefully.',
  '',
  'First, the user wants to preserve paragraph breaks in the thinking output.',
  '',
  'Second, blank rows at the *start* of the collapsed view feel buggy.',
  '',
  'Third, the right approach is to trim leading empties only.',
  '',
  'Final answer: emit a top hint with the hidden-line count.',
].join('\n');

export const Paragraphed = {
  parameters: {
    docs: {
      storyDescription:
        'Thinking with paragraph breaks. Internal blank rows are preserved; ' +
        'leading empty rows in the tail slice are trimmed; the top hint ' +
        'reports how many lines (including paragraph-break empties) live above.',
    },
  },
  render: (args: ThinkingDisplayProps) => (
    <Frame>
      <ThinkingDisplay {...args} />
    </Frame>
  ),
  args: {
    text: PARAGRAPHED_TEXT,
  } satisfies ThinkingDisplayProps,
};
