import React from 'react';
import { Box } from '../../../renderer.js';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useAllowIcons, useGlyphs } from '../../../hooks/useGlyphs.js';
import { getAgentColor } from '../../../utils/agentColors.js';
import { visibleWidth } from '../../../utils/text-width.js';
import type { WorkflowMonitorNode } from '../../../types/workflow-monitor.js';
import type { WorkflowNodeStatus } from '../../../types/workflow.js';
import {
  fitWorkflowNodeMetadata,
  repeatSuffix,
} from './workflow-node-format.js';

const STATUS_COLOR_TOKENS: Record<WorkflowNodeStatus, string> = {
  pending: 'secondary',
  running: 'info',
  paused: 'warning',
  completed: 'success',
  failed: 'error',
  aborted: 'secondary',
  skipped: 'secondary',
};

export interface WorkflowNodeRowProps {
  node: WorkflowMonitorNode;
  isSelected: boolean;
  isLast: boolean;
  width: number;
  hasApproval?: boolean;
  onSelect?: () => void;
}

export function capNodeLabel(
  label: string,
  width: number,
  depth: number
): string {
  const room = Math.max(4, width - depth * 4 - 12);
  const limit = Math.min(28, room);
  return label.length > limit
    ? `${label.slice(0, Math.max(1, limit - 3))}...`
    : label;
}

export const WorkflowNodeRow = React.memo(function WorkflowNodeRow({
  node,
  isSelected,
  isLast,
  width,
  hasApproval = false,
  onSelect,
}: WorkflowNodeRowProps) {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const { allowIcons } = useAllowIcons();
  const statusColor = getColor(
    hasApproval ? 'warning' : STATUS_COLOR_TOKENS[node.status]
  );
  const agentColor = getAgentColor(node.agentName ?? node.label, getColor);
  const connector = isLast ? glyphs.treeCorner : glyphs.treeBranch;
  const indent = (width < 30 ? '  ' : '    ').repeat(node.depth);
  const cursor = isSelected ? `${glyphs.chevron} ` : '  ';
  const icon = allowIcons
    ? hasApproval
      ? glyphs.warning
      : statusGlyph(node.status, glyphs)
    : '';
  const label =
    node.type === 'step'
      ? (node.agentName ?? node.label)
      : `[${node.type}] ${node.label}`;
  const activity = hasApproval
    ? 'needs approval'
    : node.type === 'step' && node.status === 'running'
      ? 'thinking...'
      : node.pauseReason;
  const prefix = `${cursor}${indent}${connector} ${icon}${repeatSuffix(node, glyphs)} `;
  const renderedLabel = capNodeLabel(label, width, node.depth);
  const activitySuffix = activity ? `  ${activity}` : '';
  const metadata =
    node.type === 'step'
      ? fitWorkflowNodeMetadata(
          node,
          Math.max(
            0,
            width -
              visibleWidth(prefix) -
              visibleWidth(renderedLabel) -
              visibleWidth(activitySuffix)
          ),
          glyphs.smallDot
        )
      : '';

  return (
    <Box
      width={width}
      backgroundColor={isSelected ? getColor('surface').hex : undefined}
      onClick={onSelect}
    >
      <Text wrap="truncate">
        {statusColor(prefix)}
        {agentColor(renderedLabel)}
        {activitySuffix ? statusColor(activitySuffix) : ''}
        {metadata ? getColor('secondary')(metadata) : ''}
      </Text>
    </Box>
  );
});

function statusGlyph(
  status: WorkflowNodeStatus,
  glyphs: ReturnType<typeof useGlyphs>
): string {
  switch (status) {
    case 'pending':
      return glyphs.dotEmpty;
    case 'running':
      return glyphs.executing;
    case 'paused':
      return glyphs.pause;
    case 'completed':
      return glyphs.checkmark;
    case 'failed':
      return glyphs.cross;
    case 'aborted':
    case 'skipped':
      return glyphs.times;
  }
}
