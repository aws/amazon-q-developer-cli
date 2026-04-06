import React from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';

export interface Column {
  /** Header label. Omit or set empty to hide individual header. */
  label?: string;
  /** Fixed width. Last column can omit to fill remaining space. */
  width?: number;
}

export interface Cell {
  text: string;
  color?: (s: string) => string;
}

export type Row = Cell[];

export interface TableProps {
  columns: Column[];
  rows: Row[];
  showHeaders?: boolean;
  /** Index of the row to highlight (e.g. for selection). -1 or undefined = none. */
  highlightedRow?: number;
}

export const Table: React.FC<TableProps> = ({
  columns,
  rows,
  showHeaders = true,
  highlightedRow,
}) => {
  const { getColor } = useTheme();
  const dim = getColor('secondary');
  const primary = getColor('primary');

  return (
    <Box flexDirection="column">
      {showHeaders && (
        <Box>
          {columns.map((col, i) => {
            const content = <Text>{dim(col.label ?? '')}</Text>;
            return col.width ? (
              <Box key={i} width={col.width}>
                {content}
              </Box>
            ) : (
              <React.Fragment key={i}>{content}</React.Fragment>
            );
          })}
        </Box>
      )}
      {rows.map((row, ri) => {
        const isHighlighted = ri === highlightedRow;
        return (
          <Box key={ri}>
            {row.map((cell, ci) => {
              const color = isHighlighted
                ? (s: string) => primary.inverse(s)
                : (cell.color ?? dim);
              const content = <Text>{color(cell.text)}</Text>;
              const col = columns[ci];
              return col?.width ? (
                <Box key={ci} width={col.width}>
                  {content}
                </Box>
              ) : (
                <React.Fragment key={ci}>{content}</React.Fragment>
              );
            })}
          </Box>
        );
      })}
    </Box>
  );
};
