import type { Glyphs } from '../../../utils/glyphs.js';
import { formatEffort } from '../../../utils/string.js';
import { visibleWidth } from '../../../utils/text-width.js';
import type { WorkflowMonitorNode } from '../../../types/workflow-monitor.js';

export function repeatSuffix(
  node: WorkflowMonitorNode,
  glyphs: Glyphs
): string {
  if (node.iteration === undefined) return '';
  return ` ${glyphs.loop}${node.iteration + 1}/${node.maxIterations ?? '?'}`;
}

const NODE_METADATA_GAP = '  ';

export function fitWorkflowNodeMetadata(
  node: Pick<WorkflowMonitorNode, 'modelId' | 'effortLevel'>,
  maxWidth: number,
  separator: string
): string {
  const modelId = node.modelId?.trim() || undefined;
  const effortLevel = node.effortLevel?.trim();
  const effort = effortLevel ? formatEffort(effortLevel) : undefined;
  const candidates: string[] = [];

  if (modelId && effort) {
    candidates.push(`${modelId} ${separator} ${effort}`);
  }
  if (modelId) candidates.push(modelId);
  if (effort) candidates.push(effort);

  for (const candidate of candidates) {
    const detail = `${NODE_METADATA_GAP}${candidate}`;
    if (visibleWidth(detail) <= maxWidth) return detail;
  }
  return '';
}
