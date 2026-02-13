import React from 'react';
import { Box, useInput, Text as InkText } from 'ink';
import { Text } from './text/Text.js';
import { Divider } from './divider/Divider.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useTerminalSize } from '../../hooks/useTerminalSize.js';

interface ContextBreakdownProps {
  percent: number | null;
  breakdown?: CategoryBreakdown;
  model: string | null;
  onClose: () => void;
}

interface CategoryBreakdown {
  contextFiles: { percent: number; tokens: number };
  tools: { percent: number; tokens: number };
  kiroResponses: { percent: number; tokens: number };
  yourPrompts: { percent: number; tokens: number };
  sessionFiles?: { percent: number; tokens: number };
}

// Colors matching the design
const COLORS = {
  contextFiles: '#2ee621', // Green
  tools: '#ff1dfb', // Magenta/Pink
  kiroResponses: '#723acc', // Purple
  yourPrompts: '#2c7cff', // Blue
  available: '#808080', // Gray
};

function ProgressBar({
  percent,
  breakdown,
  width,
}: {
  percent: number;
  breakdown?: CategoryBreakdown;
  width: number;
}) {
  if (!breakdown) {
    const filled = Math.round((percent / 100) * width);
    const empty = width - filled;
    return (
      <Text>
        <InkText color={COLORS.tools}>{'█'.repeat(filled)}</InkText>
        <InkText color={COLORS.available}>{'█'.repeat(empty)}</InkText>
      </Text>
    );
  }

  const minWidth = (pct: number) =>
    pct > 0 ? Math.max(1, Math.round((pct / 100) * width)) : 0;

  const contextFilesWidth = minWidth(breakdown.contextFiles.percent);
  const toolsWidth = minWidth(breakdown.tools.percent);
  const kiroWidth = minWidth(breakdown.kiroResponses.percent);
  const promptsWidth = minWidth(breakdown.yourPrompts.percent);
  const usedWidth = contextFilesWidth + toolsWidth + kiroWidth + promptsWidth;
  const emptyWidth = Math.max(0, width - usedWidth);

  return (
    <Text>
      <InkText color={COLORS.contextFiles}>
        {'█'.repeat(contextFilesWidth)}
      </InkText>
      <InkText color={COLORS.tools}>{'█'.repeat(toolsWidth)}</InkText>
      <InkText color={COLORS.kiroResponses}>{'█'.repeat(kiroWidth)}</InkText>
      <InkText color={COLORS.yourPrompts}>{'█'.repeat(promptsWidth)}</InkText>
      <InkText color={COLORS.available}>{'█'.repeat(emptyWidth)}</InkText>
    </Text>
  );
}

interface BreakdownItemProps {
  label: string;
  percent: number;
  hexColor: string;
}

function BreakdownItem({ label, percent, hexColor }: BreakdownItemProps) {
  const { getColor } = useTheme();
  return (
    <Box>
      <InkText color={hexColor}>{'█'}</InkText>
      <Text>
        {' '}
        {getColor('primary')(label)}{' '}
        {getColor('secondary')(`${percent.toFixed(1)}%`)}
      </Text>
      <Text>{'  '}</Text>
    </Box>
  );
}

export function ContextBreakdown({
  percent,
  breakdown,
  onClose,
}: ContextBreakdownProps) {
  const { getColor } = useTheme();
  const { width: termWidth } = useTerminalSize();
  const primary = getColor('primary');
  const brand = getColor('brand');
  const dim = getColor('secondary');

  const displayPercent = percent ?? 0;
  const contextLeft = 100 - displayPercent;
  const barWidth = Math.max(20, termWidth - 24);
  const [expanded, setExpanded] = React.useState(false);

  // ~18 chars per category item, 5 items = ~90 chars
  const useHorizontalLayout = termWidth >= 90;

  useInput((_input, key) => {
    if (key.escape) {
      onClose();
    }
    if (key.ctrl && _input === 'o') {
      setExpanded((e) => !e);
    }
  });

  const categories = breakdown && (
    <Box
      flexDirection={useHorizontalLayout ? 'row' : 'column'}
      justifyContent={useHorizontalLayout ? 'space-between' : undefined}
    >
      <BreakdownItem
        label="Agent files"
        percent={breakdown.contextFiles.percent}
        hexColor={COLORS.contextFiles}
      />
      <BreakdownItem
        label="Tools"
        percent={breakdown.tools.percent}
        hexColor={COLORS.tools}
      />
      <BreakdownItem
        label="Kiro responses"
        percent={breakdown.kiroResponses.percent}
        hexColor={COLORS.kiroResponses}
      />
      <BreakdownItem
        label="Your prompts"
        percent={breakdown.yourPrompts.percent}
        hexColor={COLORS.yourPrompts}
      />
    </Box>
  );

  return (
    <Box flexDirection="column" paddingX={1} paddingY={0}>
      <Text>{primary('/context')}</Text>
      <Divider />

      <Box marginBottom={1}>
        <Text>{primary('Current context window:')}</Text>
      </Box>

      <Box marginBottom={1}>
        <ProgressBar
          percent={displayPercent}
          breakdown={breakdown}
          width={barWidth}
        />
        <Text> {primary(`${contextLeft.toFixed(0)}% context left`)}</Text>
      </Box>

      <Box marginBottom={0}>{categories}</Box>

      {expanded && (
        <Box flexDirection="column" marginTop={1}>
          <Divider />
          <Text>{dim('Tips:')}</Text>
          <Text>
            {'  '}
            {brand('/compact')} {dim('Summarize conversation history')}
          </Text>
          <Text>
            {'  '}
            {brand('/clear')} {dim('Erase entire chat history')}
          </Text>
        </Box>
      )}

      <Divider />
      <Box justifyContent="space-between">
        <Text>
          {primary('ESC')} {dim('to close')}
        </Text>
        <Text>
          {primary('ctrl+o')} {dim(expanded ? 'to collapse' : 'to expand')}
        </Text>
      </Box>
    </Box>
  );
}
