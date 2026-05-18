import React from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { CURSOR_MARKER } from './../../../renderer.js';
import type { RequirementItem } from '../../../utils/spec-artifact-parser/index.js';

interface Props {
  item: RequirementItem;
  selected: boolean;
}

export const RequirementSummaryItem: React.FC<Props> = ({ item, selected }) => {
  const { getColor } = useTheme();
  const primary = getColor('primary');
  const dim = getColor('secondary');
  const accent = getColor('accent');

  // Render the user story line stripped of its `**User Story:**` prefix so
  // the summary stays compact. If the line is missing or empty we surface a
  // muted "no user story" hint rather than rendering an empty row.
  let userStoryDisplay: string;
  if (item.userStory) {
    const trimmed = item.userStory.replace(/^\s+/, '');
    userStoryDisplay = trimmed.replace(/^\*\*User Story:\*\*\s*/, '').trim();
  } else {
    userStoryDisplay = '';
  }

  const cursor = selected ? CURSOR_MARKER : ' ';
  const numberLabel = `Requirement ${item.number}`;

  return (
    <Box flexDirection="column" marginBottom={1}>
      <Box>
        <Text>{cursor}</Text>
        <Text>
          {selected ? accent.bold(numberLabel) : primary(numberLabel)}
        </Text>
        {item.title.length > 0 && (
          <>
            <Text>{primary(': ')}</Text>
            <Text>{item.title}</Text>
          </>
        )}
      </Box>
      {userStoryDisplay.length > 0 ? (
        <Box marginLeft={2}>
          <Text>{dim(userStoryDisplay)}</Text>
        </Box>
      ) : (
        <Box marginLeft={2}>
          <Text>{dim('(no user story)')}</Text>
        </Box>
      )}
    </Box>
  );
};
