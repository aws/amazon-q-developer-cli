import React from 'react';
import { Box, useInput } from 'ink';
import { Text } from './text/Text.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import type { LastTurnTokens } from '../../stores/app-store.js';
import {
  DEFAULT_CONTEXT_WINDOW,
  getUsageColor,
} from '../../utils/context-utils.js';

interface ContextBreakdownProps {
  percent: number | null;
  tokens: LastTurnTokens | null;
  model: string | null;
  onClose: () => void;
}

function ProgressBar({
  percent,
  width = 50,
}: {
  percent: number;
  width?: number;
}) {
  const { getColor } = useTheme();
  const color = getColor(getUsageColor(percent));
  const filled = Math.round((percent / 100) * width);
  const empty = width - filled;

  return (
    <Text>
      {color('█'.repeat(filled))}
      {getColor('secondary')('░'.repeat(empty))}{' '}
      {color(`${percent.toFixed(1)}%`)}
    </Text>
  );
}

interface BreakdownItemProps {
  label: string;
  percent: number;
  color: ReturnType<ReturnType<typeof useTheme>['getColor']>;
}

function BreakdownItem({ label, percent, color }: BreakdownItemProps) {
  const { getColor } = useTheme();
  return (
    <Box>
      <Text>
        {color('█')} {getColor('secondary')(label.padEnd(16))}
        {getColor('primary')(`${percent.toFixed(1)}%`)}
      </Text>
    </Box>
  );
}

export function ContextBreakdown({
  percent,
  tokens,
  model,
  onClose,
}: ContextBreakdownProps) {
  const { getColor } = useTheme();
  const dim = getColor('secondary');
  const primary = getColor('primary');
  const brand = getColor('brand');

  const displayPercent = percent ?? 0;

  useInput((_input, key) => {
    if (key.escape) {
      onClose();
    }
  });

  return (
    <Box flexDirection="column" paddingX={1} paddingY={0}>
      <Box marginBottom={1}>
        <Text>
          {primary('Context window: ')}
          {getColor(getUsageColor(displayPercent))(
            `${displayPercent.toFixed(1)}%`
          )}{' '}
          {dim('used')}
        </Text>
      </Box>

      <Box marginBottom={1}>
        <ProgressBar percent={displayPercent} width={40} />
      </Box>

      {tokens && (
        <Box flexDirection="column" marginBottom={1}>
          <BreakdownItem
            label="Input tokens"
            percent={(tokens.input / DEFAULT_CONTEXT_WINDOW) * 100}
            color={getColor('brand')}
          />
          <BreakdownItem
            label="Output tokens"
            percent={(tokens.output / DEFAULT_CONTEXT_WINDOW) * 100}
            color={getColor('success')}
          />
          {tokens.cached > 0 && (
            <BreakdownItem
              label="Cached tokens"
              percent={(tokens.cached / DEFAULT_CONTEXT_WINDOW) * 100}
              color={getColor('secondary')}
            />
          )}
        </Box>
      )}

      <Box flexDirection="column" marginTop={1}>
        <Text>{dim('Tips:')}</Text>
        <Text>
          {dim('  /compact  ')} {primary('Summarize conversation history')}
        </Text>
        <Text>
          {dim('  /clear    ')} {primary('Erase entire chat history')}
        </Text>
      </Box>

      <Box marginTop={1}>
        <Text>
          {dim('Press ')}
          {brand('Esc')}
          {dim(' to close')}
        </Text>
      </Box>
    </Box>
  );
}
