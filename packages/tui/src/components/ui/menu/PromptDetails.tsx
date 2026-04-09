import React, { useState, useCallback } from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../text/Text.js';
import { Divider } from '../divider/Divider.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useKeypress } from '../../../hooks/useKeypress.js';
import type { CommandMeta } from '../../../types/commands.js';

export interface PromptDetailsProps {
  name: string;
  description: string;
  meta?: CommandMeta;
  onBack: () => void;
  onExecute: () => void;
  /** Max content lines visible before showing (+N more). Header/footer are outside this. */
  visibleLines?: number;
}

export const PromptDetails: React.FC<PromptDetailsProps> = ({
  name,
  description,
  meta,
  onBack,
  onExecute,
  visibleLines = 8,
}) => {
  const { getColor } = useTheme();
  const dimText = getColor('secondary');
  const brandText = getColor('primary');

  const args = meta?.arguments ?? [];
  const serverName = meta?.serverName;

  const usage = args.length
    ? `${name} ${args.map((a) => (a.required ? `<${a.name}>` : `[${a.name}]`)).join(' ')}`
    : null;

  const lines: React.ReactNode[] = [];

  if (description) {
    lines.push(
      <Box key="dl">
        <Text>{dimText('Description:')}</Text>
      </Box>
    );
    lines.push(
      <Box key="d" paddingLeft={2}>
        <Text>{description}</Text>
      </Box>
    );
    lines.push(<Box key="s2" height={1} />);
  }

  if (usage) {
    lines.push(
      <Box key="u">
        <Text>
          {dimText('Usage: ')}
          {usage}
        </Text>
      </Box>
    );
    lines.push(<Box key="s3" height={1} />);
  }

  if (args.length > 0) {
    lines.push(
      <Box key="al">
        <Text>{dimText('Arguments:')}</Text>
      </Box>
    );
    for (const arg of args) {
      const req = arg.required ? '(required)' : '(optional)';
      const desc = arg.description ? ` - ${arg.description}` : '';
      lines.push(
        <Box key={`a-${arg.name}`} paddingLeft={2}>
          <Text>
            {dimText(req)} {brandText(arg.name)}
            {dimText(desc)}
          </Text>
        </Box>
      );
    }
  }

  const maxOffset = Math.max(0, lines.length - visibleLines);
  const [scrollOffset, setScrollOffset] = useState(0);

  useKeypress(
    useCallback(
      (
        _input: string,
        key: {
          escape: boolean;
          return: boolean;
          upArrow: boolean;
          downArrow: boolean;
        }
      ) => {
        if (key.escape) onBack();
        else if (key.return) onExecute();
        else if (key.upArrow) setScrollOffset((p) => Math.max(0, p - 1));
        else if (key.downArrow)
          setScrollOffset((p) => Math.min(maxOffset, p + 1));
      },
      [onBack, onExecute, maxOffset]
    )
  );

  const visibleSlice = lines.slice(scrollOffset, scrollOffset + visibleLines);
  const remaining = lines.length - scrollOffset - visibleSlice.length;

  return (
    <Box flexDirection="column">
      <Box>
        <Text>
          {dimText('Name: ')}
          {brandText(name)}
          {serverName ? (
            <>
              {dimText(' · Server: ')}
              {dimText(serverName)}
            </>
          ) : null}
        </Text>
      </Box>
      <Box height={1} />
      <Box
        flexDirection="column"
        paddingLeft={1}
        height={visibleLines}
        overflow="hidden"
      >
        {visibleSlice}
      </Box>
      {remaining > 0 ? (
        <Box paddingLeft={1}>
          <Text>{dimText(`(+${remaining} more)`)}</Text>
        </Box>
      ) : (
        <Box height={1} />
      )}
      <Divider />
      <Box paddingX={1}>
        <Text>
          {brandText('ESC')} {dimText('to go back')}
          {dimText(' · ')}
          {brandText('↑↓')} {dimText('to scroll')}
          {dimText(' · ')}
          {brandText('↵')} {dimText('to run')}
        </Text>
      </Box>
    </Box>
  );
};
