import React from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';
import { clauseRowText } from './clauseRow.js';
import { cursorColumn } from './cursorColumn.js';
import type { BugfixSection } from '../../../utils/spec-artifact-parser/index.js';

interface Props {
  section: BugfixSection;
  selected: boolean;
  expanded: boolean;
}

/**
 * One of a bugfix document's behaviour sections.
 *
 * Collapsed it is a title and a count. Expanded it gives one clipped row per
 * clause: enough to tell which clause is which, without reproducing the document
 * — reading it is what opening it is for.
 */
export const BugfixSummaryItem: React.FC<Props> = ({
  section,
  selected,
  expanded,
}) => {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const { width } = useTerminalSize();
  const primary = getColor('primary');
  const dim = getColor('secondary');
  const accent = getColor('accent');

  const count = section.clauses.length;
  const titleColor = selected ? accent.bold : primary;

  return (
    <Box flexDirection="column">
      <Box>
        <Text>{cursorColumn(selected)}</Text>
        <Text>{titleColor(section.title)}</Text>
        <Text>{dim(` (${count} clause${count === 1 ? '' : 's'})`)}</Text>
        <Text> </Text>
        <Text>{dim(expanded ? glyphs.arrowDown : glyphs.arrowRight)}</Text>
      </Box>
      {expanded && (
        <Box flexDirection="column" marginLeft={4}>
          {section.clauses.map((clause) => (
            // Number and text share one row so the row can be clipped as a
            // whole, and so nothing can come between a number and its clause.
            // `truncate-end` stays on as the guard that a row can never become
            // two, whatever the measured width.
            <Text key={clause.number} wrap="truncate-end">
              {primary(clause.number)}{' '}
              {dim(clauseRowText(clause.number, clause.text, width))}
            </Text>
          ))}
        </Box>
      )}
    </Box>
  );
};
