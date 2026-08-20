import React from 'react';
import { Box } from './../../../renderer.js';
import { ToolMeta } from './ToolMeta.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import { certifyVisualStory } from '../../../storybook/story-certification.js';

function MetaStory({
  title,
  params,
}: {
  title: string;
  params: string[] | null;
}) {
  return (
    <Box flexDirection="column">
      <StatusInfo title={title} />
      <ToolMeta params={params} />
    </Box>
  );
}

const viewport = { columns: 100, rows: 10 };

const meta = {
  component: ToolMeta,
  parameters: {
    layout: 'fullscreen',
    visualStates: {
      standard: { label: 'Standard argument list' },
      absent: { label: 'No arguments' },
      multiline: { label: 'Arguments containing multiple lines' },
      wrapped: { label: 'Arguments wrap at narrow terminal width' },
      rollout: { label: 'Rollout argument presentation' },
      'args-off': {
        label: 'Arguments hidden by display policy',
        gapType: 'integration-only',
        description: 'The display-policy provider owns argument visibility.',
      },
      'line-cap': {
        label: 'Arguments respect the display line cap',
        gapType: 'integration-only',
        description: 'The display-policy provider owns line caps.',
      },
      'character-cap': {
        label: 'Arguments respect the display character cap',
        gapType: 'integration-only',
        description: 'The display-policy provider owns character caps.',
      },
      expanded: {
        label: 'Expanded argument block',
        gapType: 'integration-only',
        description: 'Expansion is owned by the global tool-output controller.',
      },
      'static-truncation': {
        label: 'Static arguments drop interactive expansion hints',
        gapType: 'integration-only',
        description: 'Static state is supplied by ToolUseMessage policy.',
      },
    },
    storyOrder: [
      'Standard',
      'NoParameters',
      'Multiline',
      'NarrowWrap',
      'Rollout',
    ],
  },
  tags: ['autodocs'],
};

export default meta;

export const Standard = {
  render: () => (
    <MetaStory
      title="Grep"
      params={['caseSensitive=true', 'includePattern=**/*.tsx']}
    />
  ),
  parameters: certifyVisualStory(
    'includePattern',
    {
      visible: ['Grep', 'caseSensitive=true', 'includePattern=**/*.tsx'],
      ordered: ['Grep', 'caseSensitive=true', 'includePattern=**/*.tsx'],
    },
    ['standard'],
    viewport
  ),
};

export const NoParameters = {
  render: () => <MetaStory title="Read config.ts" params={null} />,
  parameters: certifyVisualStory(
    'Read config.ts',
    {
      visible: ['Read config.ts'],
      hidden: ['╰', '='],
    },
    ['absent'],
    viewport
  ),
};

export const Multiline = {
  render: () => (
    <MetaStory
      title="CustomTool"
      params={['ARG_LINE_1\nARG_LINE_2', 'ARG_LINE_3']}
    />
  ),
  parameters: certifyVisualStory(
    'ARG_LINE_3',
    {
      visible: ['CustomTool', 'ARG_LINE_1', 'ARG_LINE_2', 'ARG_LINE_3'],
      ordered: ['CustomTool', 'ARG_LINE_1', 'ARG_LINE_2', 'ARG_LINE_3'],
    },
    ['multiline'],
    viewport
  ),
};

export const NarrowWrap = {
  render: () => (
    <MetaStory
      title="Glob"
      params={['includePattern=src/**/*.tsx', 'exclude=node_modules']}
    />
  ),
  parameters: certifyVisualStory(
    'includePattern',
    {
      visible: ['Glob', 'includePattern=src/', '**/*.tsx', 'node_modules'],
      ordered: ['Glob', 'includePattern=src/', '**/*.tsx', 'node_modules'],
    },
    ['wrapped'],
    { columns: 30, rows: 10 }
  ),
};

export const Rollout = {
  render: () => (
    <MetaStory
      title="Shell"
      params={['timeout=30', 'cwd=/home/user/project']}
    />
  ),
  parameters: certifyVisualStory(
    'cwd=/home/user/project',
    {
      visible: ['Shell', 'timeout=30', 'cwd=/home/user/project'],
      ordered: ['Shell', 'timeout=30', 'cwd=/home/user/project'],
    },
    ['standard', 'rollout'],
    viewport,
    { KIRO_LITE_ROLLOUT_ENABLED: '1' }
  ),
};
