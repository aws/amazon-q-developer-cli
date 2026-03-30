import React, { useMemo } from 'react';
import { Box, Text } from '../../../renderer.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import type { Stage, StageState } from './types.js';
import { truncate } from './types.js';
import { SpinnerIcon } from './SpinnerIcon.js';

interface DagNode {
  key: string;
  depName?: string;
  depState?: StageState;
  connector?: string;
  targetName?: string;
  targetState?: StageState;
  isConnectorOnly?: boolean;
  isStandalone?: boolean;
}

export const DagVisualization = React.memo(function DagVisualization({
  stages,
  groupStages,
  selectedIndex,
  sessionsWithApproval,
}: {
  stages: Stage[];
  groupStages: Stage[];
  selectedIndex: number;
  sessionsWithApproval: Set<string>;
}) {
  const { getColor } = useTheme();

  const stageByName = useMemo(
    () => new Map(stages.map((s) => [s.name, s])),
    [stages]
  );
  const indexByName = useMemo(
    () => new Map(stages.map((s, i) => [s.name, i])),
    [stages]
  );

  const allStages = groupStages.length > 0 ? groupStages : stages;
  const maxNameLen = useMemo(
    () => Math.max(...allStages.map((s) => s.name.length), 6),
    [allStages]
  );
  const STATUS_W = 22;
  // "NN " + icon(3 terminal cols) + " " + name + " " + status + " "= fixed cell width
  const cellW = 3 + 3 + 1 + maxNameLen + 1 + STATUS_W + 1;

  const nodes: DagNode[] = useMemo(() => {
    const nameToStage = new Map(stages.map((s) => [s.name, s]));
    const result: DagNode[] = [];
    const rendered = new Set<string>();

    allStages.forEach((stage) => {
      const deps = stage.dependsOn ?? [];
      if (deps.length === 0 || rendered.has(stage.name)) return;
      rendered.add(stage.name);
      deps.forEach((d) => rendered.add(d));

      const tState = nameToStage.get(stage.name)?.state ?? 'Pending';

      if (deps.length === 1) {
        const dep0 = deps[0]!;
        const dState = nameToStage.get(dep0)?.state ?? 'Pending';
        result.push({
          key: stage.name,
          depName: dep0,
          depState: dState,
          connector: '──→',
          targetName: stage.name,
          targetState: tState,
        });
      } else {
        deps.forEach((dep, i) => {
          const dState = nameToStage.get(dep)?.state ?? 'Pending';
          const connector =
            i === 0 ? '──┐' : i === deps.length - 1 ? '──┘' : '──┤';
          result.push({
            key: `${stage.name}-${dep}`,
            depName: dep,
            depState: dState,
            connector,
          });
          if (i === Math.floor((deps.length - 1) / 2)) {
            result.push({
              key: `${stage.name}-target`,
              isConnectorOnly: true,
              targetName: stage.name,
              targetState: tState,
            });
          }
        });
      }
    });

    // Show stages with no dependencies as standalone nodes
    allStages.forEach((stage) => {
      if (!rendered.has(stage.name)) {
        rendered.add(stage.name);
        result.push({
          key: stage.name,
          targetName: stage.name,
          targetState: nameToStage.get(stage.name)?.state ?? 'Pending',
          isStandalone: true,
        });
      }
    });

    return result;
  }, [stages, allStages, selectedIndex, indexByName]);

  if (nodes.length === 0) return null;

  const isSel = (name: string) =>
    (indexByName.get(name) ?? -1) === selectedIndex;

  const getStatusInfo = (name: string) => {
    const s = stageByName.get(name);
    const text = s?.activeStatus ?? '';
    const hasPending = s && sessionsWithApproval.has(s.sessionId);
    const color = hasPending
      ? 'yellow'
      : s?.state === 'Completed'
        ? getColor('success').hex
        : s?.state === 'Failed'
          ? getColor('error').hex
          : 'gray';
    return { text: truncate(text, STATUS_W), color };
  };

  const hasPending = (name: string) => {
    const s = stageByName.get(name);
    return !!(s && sessionsWithApproval.has(s.sessionId));
  };

  /** Render a fixed-width cell. leaf=true uses spaces for padding, otherwise ─ */
  const cell = (name: string, state: StageState, leaf: boolean) => {
    const idx = indexByName.get(name) ?? -1;
    const sel = isSel(name);
    const st = getStatusInfo(name);
    const paddedName = truncate(name, maxNameLen).padEnd(maxNameLen);
    const statusText = st.text;
    const fillLen = Math.max(0, STATUS_W - statusText.length);
    const fillChar = leaf ? ' ' : '─';
    const fill = fillChar.repeat(fillLen);

    return (
      <>
        <Text color="gray">{(idx + 1).toString().padStart(2)} </Text>
        {hasPending(name) ? (
          <Text color="yellow">⚠</Text>
        ) : (
          <SpinnerIcon state={state} />
        )}
        <Text bold={sel}>
          {' '}
          <Text color="gray" underline={sel} bold={sel}>{paddedName}</Text>{' '}
          <Text color={st.color}>{statusText}</Text>{' '}
          <Text color="gray">{fill}</Text>
        </Text>
      </>
    );
  };

  return (
    <Box flexDirection="column" marginTop={1}>
      <Box paddingX={1}>
        <Text bold color="white">
          SUBAGENTS
        </Text>
        <Text color="gray">
          {'  [/] ←→ select · 1-'}
          {Math.min(9, stages.length)}
          {' jump'}
        </Text>
      </Box>
      <Box justifyContent="flex-start" paddingX={1}>
        <Box flexDirection="column">
          {nodes.map((n) => {
            if (n.isStandalone) {
              return (
                <Box key={n.key}>
                  {cell(n.targetName!, n.targetState!, true)}
                </Box>
              );
            }
            if (n.isConnectorOnly) {
              return (
                <Box key={n.key}>
                  <Text color="gray">{' '.repeat(cellW)}├──→ </Text>
                  {cell(n.targetName!, n.targetState!, true)}
                </Box>
              );
            }
            if (n.targetName) {
              return (
                <Box key={n.key}>
                  {cell(n.depName!, n.depState!, false)}
                  <Text color="gray">──→ </Text>
                  {cell(n.targetName, n.targetState!, true)}
                </Box>
              );
            }
            return (
              <Box key={n.key}>
                {cell(n.depName!, n.depState!, false)}
                <Text color="gray">{n.connector}</Text>
              </Box>
            );
          })}
        </Box>
      </Box>
    </Box>
  );
});
