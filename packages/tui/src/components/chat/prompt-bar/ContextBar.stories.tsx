import React from 'react';
import { ContextBar } from './ContextBar.js';
import { Chip, ChipColor } from '../../ui/chip/index.js';

const meta = {
  component: ContextBar,
  parameters: {
    layout: 'fullscreen',
    storyOrder: ['FullInfo', 'NoGit', 'Minimal', 'CustomChips'],
  },
  tags: ['autodocs'],
};

export default meta;

export const FullInfo = {
  parameters: {
    docs: {
      storyDescription:
        'Complete command bar with workspace, git, and model info',
    },
  },
  render: () => (
    <ContextBar>
      <Chip value="~/developer/my-project" color={ChipColor.BRAND} />
      <Chip value="main" color={ChipColor.PRIMARY} prefix="git:" wrap={true} />
      <Chip value="claude-3.5-sonnet" color={ChipColor.PRIMARY} />
    </ContextBar>
  ),
};

export const NoGit = {
  parameters: {
    docs: {
      storyDescription: 'Command bar without git information',
    },
  },
  render: () => (
    <ContextBar>
      <Chip value="~/developer/my-project" color={ChipColor.BRAND} />
      <Chip value="gpt-4" color={ChipColor.PRIMARY} />
    </ContextBar>
  ),
};

export const Minimal = {
  parameters: {
    docs: {
      storyDescription: 'Minimal command bar with only model information',
    },
  },
  render: () => (
    <ContextBar>
      <Chip value="claude-3.5-sonnet" color={ChipColor.PRIMARY} />
    </ContextBar>
  ),
};

export const CustomChips = {
  parameters: {
    docs: {
      storyDescription: 'Command bar with custom chip configurations',
    },
  },
  render: () => (
    <ContextBar>
      <Chip value="online" color={ChipColor.SUCCESS} prefix="● " />
      <Chip
        value="feature/new-ui"
        color={ChipColor.PRIMARY}
        prefix="git:"
        wrap={true}
      />
      <Chip value="processing" color={ChipColor.WARNING} prefix="⟳ " />
      <Chip value="claude-3.5-sonnet" color={ChipColor.PRIMARY} />
    </ContextBar>
  ),
};
