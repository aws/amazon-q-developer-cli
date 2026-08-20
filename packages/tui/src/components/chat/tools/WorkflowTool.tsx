import React, { useMemo } from 'react';
import { Box } from '../../../renderer.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { ToolUseStatus, type ToolResult } from '../../../stores/app-store.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import { Text } from '../../ui/text/Text.js';

export interface WorkflowToolProps {
  name: string;
  isFinished?: boolean;
  content?: string;
  result?: ToolResult;
  status?: ToolUseStatus;
}

type JsonRecord = Record<string, unknown>;

function isRecord(value: unknown): value is JsonRecord {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function parsePayload(value: unknown): JsonRecord | null {
  if (typeof value === 'string') {
    try {
      return parsePayload(JSON.parse(value) as unknown);
    } catch {
      return null;
    }
  }
  if (!isRecord(value)) return null;

  const firstItem = Array.isArray(value.items) ? value.items[0] : undefined;
  if (isRecord(firstItem) && firstItem.Json !== undefined) {
    return parsePayload(firstItem.Json);
  }
  return value;
}

function workflowDefinition(payload: JsonRecord): JsonRecord {
  return isRecord(payload.workflow) ? payload.workflow : payload;
}

function firstString(
  record: JsonRecord,
  keys: readonly string[]
): string | null {
  for (const key of keys) {
    const value = record[key];
    if (typeof value === 'string' && value.length > 0) return value;
  }
  return null;
}

function workflowName(payload: JsonRecord | null): string | null {
  if (!payload) return null;
  const definition = workflowDefinition(payload);
  return (
    firstString(payload, [
      'name',
      'workflowName',
      'workflowId',
      'workflowPath',
    ]) ??
    firstString(definition, ['name', 'workflowName', 'workflowId']) ??
    (typeof payload.workflow === 'string' ? payload.workflow : null)
  );
}

function countStepNodes(value: unknown): number {
  if (!Array.isArray(value)) return 0;
  return value.reduce((total, candidate) => {
    if (!isRecord(candidate)) return total;
    if (candidate.type === 'step') return total + 1;
    return (
      total +
      countStepNodes(candidate.steps) +
      countStepNodes(candidate.branches) +
      countStepNodes(candidate.nodes) +
      countStepNodes(candidate.nodeTree)
    );
  }, 0);
}

function workflowStepCount(payload: JsonRecord | null): number | null {
  if (!payload) return null;
  const definition = workflowDefinition(payload);
  const candidates = [
    definition.steps,
    definition.nodes,
    definition.nodeTree,
    payload.steps,
    payload.nodes,
    payload.nodeTree,
  ];
  for (const candidate of candidates) {
    if (!Array.isArray(candidate)) continue;
    const count = countStepNodes(candidate);
    return count || candidate.length || null;
  }
  return null;
}

function normalizeToolName(name: string): string {
  return name.toLowerCase().trim().replace(/\s+/g, '_');
}

export const WorkflowTool = React.memo(function WorkflowTool({
  name,
  isFinished = false,
  content,
  result,
  status,
}: WorkflowToolProps) {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const input = useMemo(() => parsePayload(content), [content]);
  const output = useMemo(
    () => (result?.status === 'success' ? parsePayload(result.output) : null),
    [result]
  );
  const displayName = workflowName(input) ?? workflowName(output);
  const stepCount =
    workflowStepCount(input) ?? (isFinished ? workflowStepCount(output) : null);
  const stepLabel =
    stepCount == null ? null : `${stepCount} step${stepCount === 1 ? '' : 's'}`;
  const cancelled =
    status === ToolUseStatus.Rejected || result?.status === 'cancelled';
  const normalizedName = normalizeToolName(name);

  if (normalizedName === 'inspect_workflow') {
    if (cancelled) {
      return (
        <StatusInfo
          title={`${glyphs.cross} Workflow inspection cancelled`}
          target={displayName ? `"${displayName}"` : undefined}
          status="warning"
          useStatusColor
        />
      );
    }
    if (result?.status === 'error') {
      return (
        <Box flexDirection="column">
          <StatusInfo
            title={`${glyphs.cross} Workflow inspection failed`}
            target={displayName ? `"${displayName}"` : undefined}
            status="error"
            useStatusColor
          />
          <Box marginLeft={2}>
            <Text>{getColor('error')(result.error)}</Text>
          </Box>
        </Box>
      );
    }
    return (
      <StatusInfo
        title={`${glyphs.eye} ${
          isFinished ? 'Checked workflow status' : 'Checking workflow status'
        }`}
        target={displayName ? `"${displayName}"` : undefined}
        shimmer={!isFinished}
      />
    );
  }

  if (cancelled) {
    return (
      <StatusInfo
        title={`${glyphs.cross} Workflow start cancelled`}
        target={displayName ? `"${displayName}"` : undefined}
        status="warning"
        useStatusColor
      />
    );
  }

  if (result?.status === 'error') {
    return (
      <Box flexDirection="column">
        <StatusInfo
          title={`${glyphs.cross} Workflow failed to start`}
          target={displayName ? `"${displayName}"` : undefined}
          status="error"
          useStatusColor
        />
        <Box marginLeft={2}>
          <Text>{getColor('error')(result.error)}</Text>
        </Box>
      </Box>
    );
  }

  if (!isFinished) {
    const target = [displayName ? `"${displayName}"` : null, stepLabel]
      .filter((value): value is string => value !== null)
      .join(' ');
    return (
      <StatusInfo
        title={`${glyphs.triangleRight} Starting workflow`}
        target={target || undefined}
        shimmer
      />
    );
  }

  const details = [
    displayName ? `"${displayName}"` : null,
    stepLabel,
    `ctrl+g monitor`,
  ].filter((value): value is string => value !== null);
  return (
    <StatusInfo
      title={`${glyphs.triangleRight} Started workflow`}
      target={details.join(` ${glyphs.smallDot} `)}
    />
  );
});
