import React, { useMemo, useState } from 'react';
import { Box, useInput } from './../../../renderer.js';
import { Text } from '../../ui/text/Text.js';
import { StatusBar, useStatusBar } from '../status-bar/StatusBar.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';
import { expandTabs } from '../../../utils/string.js';
import type { StatusType } from '../../../types/componentTypes.js';

const HEAD_LINES = 5;
const MAX_EXPANDED_LINES = 1000;

interface ShellOutputMessageProps {
  content: string;
  isStatic?: boolean;
  status?: StatusType;
  barColor?: string;
}

export const ShellOutputMessage = React.memo(function ShellOutputMessage({
  content,
  isStatic = false,
  status,
  barColor,
}: ShellOutputMessageProps) {
  return (
    <StatusBar status={status || 'active'} barColor={barColor}>
      <ShellOutputContent content={content} isStatic={isStatic} />
    </StatusBar>
  );
});

function ShellOutputContent({
  content,
  isStatic,
}: {
  content: string;
  isStatic: boolean;
}) {
  const { getColor } = useTheme();
  const { height: termHeight } = useTerminalSize();
  const { requestRemeasure } = useStatusBar();
  const primaryColor = getColor('primary');
  const secondaryColor = getColor('secondary');

  const [expanded, setExpanded] = useState(false);

  const lines = useMemo(() => expandTabs(content).split('\n'), [content]);
  const tailLines = Math.max(5, termHeight - 10);
  const hasMore = lines.length > HEAD_LINES + tailLines;

  useInput(
    (_input, key) => {
      if (key.ctrl && _input === 'o' && hasMore) {
        setExpanded((prev) => !prev);
        requestRemeasure();
      }
    },
    { isActive: !isStatic && hasMore }
  );

  const renderLines = useMemo(() => {
    if (!hasMore) return lines;

    if (expanded) {
      if (lines.length <= MAX_EXPANDED_LINES) return lines;
      return lines.slice(0, MAX_EXPANDED_LINES);
    }

    const head = lines.slice(0, HEAD_LINES);
    const tail = lines.slice(-tailLines);
    return { head, tail, hidden: lines.length - HEAD_LINES - tailLines };
  }, [lines, hasMore, expanded, tailLines]);

  return (
    <Box flexDirection="column">
      {Array.isArray(renderLines) ? (
        <>
          {renderLines.map((line, i) => (
            <Text key={i}>{primaryColor(line)}</Text>
          ))}
          {expanded && lines.length > MAX_EXPANDED_LINES && (
            <Text>
              {secondaryColor(
                `[truncated, showing ${MAX_EXPANDED_LINES} of ${lines.length} lines]`
              )}
            </Text>
          )}
        </>
      ) : (
        <>
          {renderLines.head.map((line, i) => (
            <Text key={`h${i}`}>{primaryColor(line)}</Text>
          ))}
          <Text>
            {secondaryColor(
              `... [${renderLines.hidden} lines hidden${isStatic ? '' : ', ctrl+o to expand'}] ...`
            )}
          </Text>
          {renderLines.tail.map((line, i) => (
            <Text key={`t${i}`}>{primaryColor(line)}</Text>
          ))}
        </>
      )}
    </Box>
  );
}
