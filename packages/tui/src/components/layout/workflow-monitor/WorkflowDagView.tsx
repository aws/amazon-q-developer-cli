import React from 'react';
import { Box } from '../../../renderer.js';
import type { WorkflowMonitorNode } from '../../../types/workflow-monitor.js';
import { WorkflowNodeRow } from './WorkflowNodeRow.js';

interface WorkflowDagViewProps {
  nodes: readonly WorkflowMonitorNode[];
  selectedIndex: number;
  width: number;
  height: number;
  approvalSessionIds?: ReadonlySet<string>;
  onSelectNode?: (index: number) => void;
}

function isLastSibling(
  node: WorkflowMonitorNode,
  index: number,
  nodes: readonly WorkflowMonitorNode[]
): boolean {
  for (let nextIndex = index + 1; nextIndex < nodes.length; nextIndex += 1) {
    const sibling = nodes[nextIndex];
    if (!sibling) continue;
    if (sibling.depth < node.depth) return true;
    if (sibling.depth === node.depth && sibling.parentId === node.parentId) {
      return false;
    }
  }
  return true;
}

export const WorkflowDagView = React.memo(function WorkflowDagView({
  nodes,
  selectedIndex,
  width,
  height,
  approvalSessionIds,
  onSelectNode,
}: WorkflowDagViewProps) {
  const visibleCount = Math.max(1, height);
  const scrollOffset = Math.max(
    0,
    Math.min(
      selectedIndex - Math.floor(visibleCount / 2),
      Math.max(0, nodes.length - visibleCount)
    )
  );
  const visibleNodes = nodes.slice(scrollOffset, scrollOffset + visibleCount);

  return (
    <Box flexDirection="column" height={height}>
      {visibleNodes.map((node, index) => {
        const globalIndex = scrollOffset + index;
        return (
          <WorkflowNodeRow
            key={`${node.id}:${node.sessionId ?? globalIndex}`}
            node={node}
            isSelected={globalIndex === selectedIndex}
            isLast={isLastSibling(node, globalIndex, nodes)}
            width={width}
            hasApproval={
              !!node.sessionId && approvalSessionIds?.has(node.sessionId)
            }
            onSelect={
              onSelectNode ? () => onSelectNode(globalIndex) : undefined
            }
          />
        );
      })}
    </Box>
  );
});
