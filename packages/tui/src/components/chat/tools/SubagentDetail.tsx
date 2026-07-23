import React, { useMemo } from 'react';
import { chalk } from '../../../utils/color.js';
import { Box } from './../../../renderer.js';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';
import { getAgentColor } from '../../../utils/agentColors.js';
import type { VerboseDisplayConfig } from '../../../lite/verbose.js';
import {
  buildRenderTheme,
  clipVisibleWidth,
  renderSubagentDigestRows,
  type SubagentStageSummary,
} from '../../../lite/render.js';
import { useExpandableOutput } from '../../../hooks/useExpandableOutput.js';
import { orderSubagentStageItems } from '../../../utils/subagent-display.js';
import { maxVisibleWidth } from '../../../utils/text-width.js';
import {
  parseSubagentArgs,
  substituteStagePrompt,
  selectSubagentDigests,
  type RenderableDigest,
  type SubagentStage,
} from './subagent-detail.js';

export interface SubagentDetailProps {
  content?: string;
  summaries: readonly SubagentStageSummary[];
  display: VerboseDisplayConfig;
  finished: boolean;
  isStatic?: boolean;
  showFullOutput: boolean;
  isKas: boolean;
}

const EMPTY_DIGESTS = selectSubagentDigests([]);

export const SubagentDetail = React.memo(function SubagentDetail({
  content,
  summaries,
  display,
  finished,
  isStatic = false,
  showFullOutput,
  isKas,
}: SubagentDetailProps) {
  const sub = display.subagent;
  const showPersistedDigests = !isStatic || display.persistOutput;
  const portActive = process.env.KIRO_LITE_ROLLOUT_ENABLED === '1';

  const { task, stages } = useMemo(() => parseSubagentArgs(content), [content]);
  const stageNames = useMemo(
    () => stages.map((stage, index) => stage.name || `stage-${index + 1}`),
    [stages]
  );

  const shouldSelectDigests =
    portActive &&
    finished &&
    showPersistedDigests &&
    (showFullOutput || (sub.responses && !isKas));
  const digests = useMemo(() => {
    if (!shouldSelectDigests) return EMPTY_DIGESTS;
    return selectSubagentDigests(
      orderSubagentStageItems(summaries, stageNames)
    );
  }, [shouldSelectDigests, summaries, stageNames]);
  const showResponses = showFullOutput && digests.responses.length > 0;
  const showRawOutput = showFullOutput && digests.rawOutput.length > 0;
  const showSummaries = sub.responses && !isKas && digests.summaries.length > 0;

  const showPipeline = portActive && sub.pipeline && stages.length > 0;
  if (!showPipeline && !showResponses && !showRawOutput && !showSummaries) {
    return null;
  }

  return (
    <Box flexDirection="column" marginLeft={2}>
      {showPipeline && (
        <PipelineTree
          stages={stages}
          task={task}
          showRoles={sub.roles}
          showDeps={sub.deps}
          showPrompts={sub.prompts}
        />
      )}
      {showResponses && (
        <DigestSection
          header={digests.responses.length === 1 ? 'response:' : 'responses:'}
          digests={digests.responses}
          isStatic={isStatic}
        />
      )}
      {showRawOutput && (
        <DigestSection
          header="full output:"
          digests={digests.rawOutput}
          isStatic={isStatic}
        />
      )}
      {showSummaries && (
        <DigestSection
          header="response summary:"
          digests={digests.summaries}
          isStatic={isStatic}
        />
      )}
    </Box>
  );
});

const PipelineTree = React.memo(function PipelineTree({
  stages,
  task,
  showRoles,
  showDeps,
  showPrompts,
}: {
  stages: SubagentStage[];
  task: string | null;
  showRoles: boolean;
  showDeps: boolean;
  showPrompts: boolean;
}) {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const dim = getColor('secondary').dim ?? getColor('secondary');
  const primary = getColor('primary');
  const promptColor = (text: string) => chalk.reset(primary(text));

  return (
    <Box flexDirection="column">
      <Text>{promptColor('pipeline:')}</Text>
      {stages.map((stage, i) => {
        const isLast = i === stages.length - 1;
        const branch = isLast
          ? `${glyphs.cornerBottomLeft}${glyphs.lineHorizontal}`
          : `${glyphs.teeRight}${glyphs.lineHorizontal}`;
        const stem = isLast ? '  ' : `${glyphs.lineVertical} `;
        const name = stage.name || `stage-${i + 1}`;
        const stageColor = getAgentColor(name, getColor);
        const prompt = showPrompts
          ? substituteStagePrompt(stage.prompt_template, task)
          : undefined;
        const hasDeps =
          showDeps &&
          Array.isArray(stage.depends_on) &&
          stage.depends_on.length > 0;

        return (
          <Box key={`${name}-${i}`} flexDirection="column">
            <Text>
              {dim(`${branch} `)}
              {stageColor(`[${name}]`)}
              {showRoles && stage.role ? dim(` (${stage.role})`) : ''}
              {hasDeps ? dim(` ← ${stage.depends_on!.join(', ')}`) : ''}
            </Text>
            {prompt && prompt.length > 0 && (
              <Box flexDirection="row">
                <Text>{dim(`${stem} `)}</Text>
                <Box flexGrow={1} flexShrink={1}>
                  <Text wrap="wrap">{promptColor(prompt)}</Text>
                </Box>
              </Box>
            )}
          </Box>
        );
      })}
    </Box>
  );
});

const DigestSection = React.memo(function DigestSection({
  header,
  digests,
  isStatic,
}: {
  header: string;
  digests: RenderableDigest[];
  isStatic: boolean;
}) {
  const { getColor } = useTheme();
  const dim = getColor('secondary').dim ?? getColor('secondary');

  return (
    <Box flexDirection="column">
      <Text>{dim(header)}</Text>
      {digests.map((digest, i) => {
        const chip = getAgentColor(digest.stageName, getColor);
        const styledChip = chip.bold ?? chip;
        return (
          <Box
            key={`${digest.stageName}-${i}`}
            flexDirection="column"
            marginLeft={2}
          >
            <Text>{styledChip(`▸ ${digest.stageName}`)}</Text>
            <DigestBody digest={digest} isStatic={isStatic} />
          </Box>
        );
      })}
    </Box>
  );
});

const DigestBody = React.memo(function DigestBody({
  digest,
  isStatic,
}: {
  digest: RenderableDigest;
  isStatic: boolean;
}) {
  const { getColor, getUserPromptColor, getUserPromptBgHex } = useTheme();
  const glyphs = useGlyphs();
  const { width: termWidth } = useTerminalSize();
  const dim = getColor('secondary').dim ?? getColor('secondary');
  const theme = useMemo(
    () => buildRenderTheme(getColor, getUserPromptColor, getUserPromptBgHex),
    [getColor, getUserPromptColor, getUserPromptBgHex]
  );
  const rows = useMemo(
    () =>
      renderSubagentDigestRows(digest.body, Math.max(20, termWidth - 6), {
        glyphs,
        theme,
      }),
    [digest.body, glyphs, termWidth, theme]
  );
  const {
    expanded,
    hiddenCount,
    expandHint,
    effectivePreviewCount,
    outputMaxChars,
  } = useExpandableOutput({
    totalItems: rows.length,
    previewCount: rows.length,
    maxContentWidth: maxVisibleWidth(rows),
    isStatic,
    unit: 'lines',
    applyVerbosityOutputCap: true,
  });
  const visibleRows = expanded ? rows : rows.slice(0, effectivePreviewCount);
  const hiddenNote = !expanded
    ? expandHint || (hiddenCount > 0 ? `...+${hiddenCount} lines` : '')
    : '';

  return (
    <Box marginLeft={2} flexDirection="column">
      {visibleRows.map((row, index) => (
        <Text key={index} wrap="overflow">
          {outputMaxChars != null && outputMaxChars > 0
            ? clipVisibleWidth(row, outputMaxChars)
            : row}
        </Text>
      ))}
      {hiddenNote && <Text>{dim(hiddenNote)}</Text>}
    </Box>
  );
});
