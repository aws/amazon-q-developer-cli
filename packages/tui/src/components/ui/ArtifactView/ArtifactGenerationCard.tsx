import React from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useGlyphs, useAllowAnimations } from '../../../hooks/useGlyphs.js';
import { useAppStore } from '../../../stores/app-store.js';
import type {
  ArtifactGenerationEntry,
  ArtifactKind,
} from '../../../stores/app-store.js';
import {
  DESIGN_OVERVIEW_MAX_CHARS,
  type ArtifactSummary,
} from '../../../utils/spec-artifact-parser/types.js';

const KIND_LABELS: Record<ArtifactKind, string> = {
  requirements: 'Requirements',
  design: 'Design',
  tasks: 'Tasks',
};

/**
 * Generation-phase card.
 *
 * Renders the active `artifactGenerating` entry, if any. The store
 * holds at most one — switching to a new artifact replaces the prior
 * entry — so the card is read-only and summarises what the agent has
 * written so far (replacing the streamed markdown noise in the chat
 * transcript). Transitions to a "complete" state after the 2 s idle
 * timer fires or on `ToolCallFinished`.
 *
 * Read-only is intentional: the user opens the structured view via
 * `/spec view <feature> [artifact]` rather than clicking through the
 * generation card.
 */
export const ArtifactGenerationCard: React.FC = () => {
  const entry = useAppStore((s) => s.artifactGenerating);
  if (!entry) return null;

  return (
    <Box flexDirection="column" marginTop={1}>
      <SingleCard entry={entry} />
    </Box>
  );
};

const SingleCard: React.FC<{ entry: ArtifactGenerationEntry }> = ({
  entry,
}) => {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const { allowAnimations } = useAllowAnimations();
  const primary = getColor('primary');
  const dim = getColor('secondary');
  const accent = getColor('accent');
  const success = getColor('success');
  const warning = getColor('warning');

  const indicator = entry.complete
    ? success(glyphs.checkmark)
    : allowAnimations
      ? accent(glyphs.executing)
      : accent(glyphs.dotFilled);

  const status = entry.complete ? 'complete' : 'writing';
  const label = `${KIND_LABELS[entry.artifact]} · ${entry.featureName}`;

  return (
    <Box
      flexDirection="column"
      paddingX={1}
      borderStyle="round"
      borderColor={primary.hex}
      marginBottom={1}
    >
      <Box>
        <Text>{indicator}</Text>
        <Text> </Text>
        <Text>{primary(label)}</Text>
        <Text>{dim(` · ${status}`)}</Text>
        {entry.parseError && (
          <Text>{warning(`  ${glyphs.warning} parse failed`)}</Text>
        )}
      </Box>
      <SummaryStrip summary={entry.summary} />
    </Box>
  );
};

/**
 * One-line summary strip describing what the parser has captured so far.
 *
 * Distinct from the full `SummaryView` panel: this is read-only, takes
 * minimum vertical space, and never shows item-level detail.
 */
const SummaryStrip: React.FC<{ summary: ArtifactSummary | null }> = ({
  summary,
}) => {
  const { getColor } = useTheme();
  const dim = getColor('secondary');
  if (!summary) {
    return (
      <Box marginTop={0}>
        <Text>{dim('  (loading…)')}</Text>
      </Box>
    );
  }

  if (summary.kind === 'requirements') {
    const n = summary.items.length;
    return (
      <Box>
        <Text>{dim(`  ${n} requirement${n === 1 ? '' : 's'}`)}</Text>
      </Box>
    );
  }

  if (summary.kind === 'design') {
    const sectionCount = summary.sections.length;
    const hasOverview = summary.overview.length > 0;
    // Show a short slice of the overview when present, otherwise just the
    // section count. We cap at 80 chars regardless of the parser's 600-char
    // truncation so the strip stays single-line on narrow terminals.
    if (hasOverview) {
      const maxStrip = Math.min(80, DESIGN_OVERVIEW_MAX_CHARS);
      const slice =
        summary.overview.length > maxStrip
          ? summary.overview.slice(0, maxStrip).trimEnd() + '…'
          : summary.overview;
      return (
        <Box>
          <Text>{dim('  ')}</Text>
          <Text>{dim(slice)}</Text>
          <Text>{dim(` · ${sectionCount} sections`)}</Text>
        </Box>
      );
    }
    return (
      <Box>
        <Text>
          {dim(`  ${sectionCount} section${sectionCount === 1 ? '' : 's'}`)}
        </Text>
      </Box>
    );
  }

  // tasks
  const taskCount = summary.items.length;
  const checked = summary.items.filter((t) => t.checked).length;
  const subTotal = summary.items.reduce((acc, t) => acc + t.subTasks.length, 0);
  return (
    <Box>
      <Text>
        {dim(
          `  ${taskCount} task${taskCount === 1 ? '' : 's'} · ${checked}/${taskCount} done · ${subTotal} sub-task${subTotal === 1 ? '' : 's'}`
        )}
      </Text>
    </Box>
  );
};
