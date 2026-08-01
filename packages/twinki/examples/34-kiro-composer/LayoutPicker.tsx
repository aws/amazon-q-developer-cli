import React from 'react';
import { Box, Text } from 'twinki';
import type { ShowcaseTheme } from '../32-acp-showcase/themes.js';
import type { LayoutChoice } from './layout.js';

export function LayoutPicker({
  layouts,
  selected,
  columns,
  rows,
  theme,
  onHighlight,
  onSelect,
  onCreate,
}: {
  layouts: LayoutChoice[];
  selected: number;
  columns: number;
  rows: number;
  theme: ShowcaseTheme;
  onHighlight: (index: number) => void;
  onSelect: (index: number) => void;
  onCreate: () => void;
}): React.ReactElement {
  const width = Math.min(54, Math.max(30, columns - 6));
  const options = layouts.length + 1;
  const height = Math.min(rows - 2, options + 4);
  return (
    <Box
      position="absolute"
      left={Math.max(0, Math.floor((columns - width) / 2))}
      top={Math.max(0, Math.floor((rows - height) / 2))}
      width={width}
      flexDirection="column"
      borderStyle="round"
      borderColor={theme.accent}
      borderTitle="LAYOUTS"
      borderTitleColor={theme.accent}
      backgroundColor={theme.panel}
      paddingY={1}
    >
      {layouts.map((layout, index) => {
        const active = index === selected;
        return (
          <Box
            key={layout.path}
            paddingX={1}
            backgroundColor={active ? theme.accent : theme.panel}
            onMouseEnter={() => onHighlight(index)}
            onClick={() => onSelect(index)}
          >
            <Text
              color={active ? theme.accentText : theme.fg}
              backgroundColor={active ? theme.accent : theme.panel}
              bold={active}
              wrap="truncate"
            >
              {`${active ? '> ' : '  '}${layout.spec.title}`}
            </Text>
          </Box>
        );
      })}
      <Box
        paddingX={1}
        backgroundColor={selected === layouts.length ? theme.accent : theme.panel}
        onMouseEnter={() => onHighlight(layouts.length)}
        onClick={onCreate}
      >
        <Text color={selected === layouts.length ? theme.accentText : theme.warning} bold>
          {`${selected === layouts.length ? '> ' : '  '}Create with Kiro...`}
        </Text>
      </Box>
    </Box>
  );
}
