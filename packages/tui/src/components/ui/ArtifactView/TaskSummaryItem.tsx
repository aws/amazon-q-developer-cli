import React from 'react';
import { Box, CURSOR_MARKER } from './../../../renderer.js';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import type { HighLevelTask } from '../../../utils/spec-artifact-parser/index.js';

interface Props {
  task: HighLevelTask;
  selected: boolean;
  expanded: boolean;
}

export const TaskSummaryItem: React.FC<Props> = ({
  task,
  selected,
  expanded,
}) => {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const primary = getColor('primary');
  const dim = getColor('secondary');
  const accent = getColor('accent');
  const success = getColor('success');

  const cursor = selected ? CURSOR_MARKER : ' ';
  // Use checkmark/dotEmpty from glyphs so ASCII mode falls back automatically.
  const box = task.checked ? glyphs.checkmark : glyphs.dotEmpty;
  const label = task.number ? `${task.number}. ${task.title}` : task.title;

  // Sub-task count is shown verbatim; sub-task text is only revealed on
  // expansion to keep the summary compact.
  const subCount = task.subTasks.length;
  const subSummary =
    subCount === 0 ? '' : ` (${subCount} sub-task${subCount === 1 ? '' : 's'})`;
  const expandHint =
    subCount > 0 ? (expanded ? glyphs.arrowDown : glyphs.arrowRight) : ' ';

  const titleColor = selected ? accent.bold : primary;

  return (
    <Box flexDirection="column">
      <Box>
        <Text>{cursor}</Text>
        <Text>{task.checked ? success(box) : dim(box)}</Text>
        <Text> </Text>
        <Text>{titleColor(label)}</Text>
        {subSummary.length > 0 && <Text>{dim(subSummary)}</Text>}
        {subCount > 0 && (
          <>
            <Text> </Text>
            <Text>{dim(expandHint)}</Text>
          </>
        )}
      </Box>
      {expanded && subCount > 0 && (
        <Box flexDirection="column" marginLeft={4}>
          {task.subTasks.map((s, i) => (
            <Box key={i}>
              <Text>
                {s.checked ? success(glyphs.checkmark) : dim(glyphs.dotEmpty)}
              </Text>
              <Text> </Text>
              <Text>{dim(s.title)}</Text>
            </Box>
          ))}
        </Box>
      )}
    </Box>
  );
};
