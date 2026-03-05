import React from 'react';
import { ContextBar } from './ContextBar.js';
import { Chip, ChipColor } from '../../ui/chip/index.js';

const meta = {
  component: ContextBar,
  parameters: {
    layout: 'fullscreen',
    storyOrder: [
      'FullInfo',
      'NoGit',
      'Minimal',
      'CustomChips',
      'PrimaryAndSecondary',
    ],
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
    <ContextBar
      primaryItems={[
        <Chip value="~/developer/my-project" color={ChipColor.BRAND} />,
      ]}
      secondaryItems={[
        <Chip
          value="main"
          color={ChipColor.PRIMARY}
          prefix="git:"
          wrap={true}
        />,
        <Chip value="claude-3.5-sonnet" color={ChipColor.PRIMARY} />,
      ]}
    />
  ),
};

export const NoGit = {
  parameters: {
    docs: {
      storyDescription: 'Command bar without git information',
    },
  },
  render: () => (
    <ContextBar
      primaryItems={[
        <Chip value="~/developer/my-project" color={ChipColor.BRAND} />,
      ]}
      secondaryItems={[<Chip value="gpt-4" color={ChipColor.PRIMARY} />]}
    />
  ),
};

export const Minimal = {
  parameters: {
    docs: {
      storyDescription: 'Minimal command bar with only model information',
    },
  },
  render: () => (
    <ContextBar
      primaryItems={[
        <Chip value="claude-3.5-sonnet" color={ChipColor.PRIMARY} />,
      ]}
    />
  ),
};

export const CustomChips = {
  parameters: {
    docs: {
      storyDescription: 'Command bar with custom chip configurations',
    },
  },
  render: () => (
    <ContextBar
      primaryItems={[
        <Chip value="online" color={ChipColor.SUCCESS} prefix="● " />,
      ]}
      secondaryItems={[
        <Chip
          value="feature/new-ui"
          color={ChipColor.PRIMARY}
          prefix="git:"
          wrap={true}
        />,
        <Chip value="processing" color={ChipColor.WARNING} prefix="⟳ " />,
        <Chip value="claude-3.5-sonnet" color={ChipColor.PRIMARY} />,
      ]}
    />
  ),
};

export const PrimaryAndSecondary = {
  parameters: {
    docs: {
      storyDescription:
        'Demonstrates primary items on left, secondary on right',
    },
  },
  render: () => (
    <ContextBar
      primaryItems={[
        <Chip value="my-agent" color={ChipColor.PRIMARY} prefix="agent:" />,
        <Chip value="45%" color={ChipColor.SUCCESS} prefix="context:" />,
      ]}
      secondaryItems={[
        <Chip value="~/project" color={ChipColor.BRAND} />,
        <Chip value="main" color={ChipColor.PRIMARY} prefix="git:" />,
        <Chip value="claude-3.5-sonnet" color={ChipColor.PRIMARY} />,
      ]}
    />
  ),
};
