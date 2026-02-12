import React, { useMemo } from 'react';
import { Box } from 'ink';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import { parseToolArg, getResultSummary } from '../../../utils/tool-result.js';
import type { ToolResult } from '../../../stores/app-store.js';

export interface WebFetchProps {
  /** Whether the fetch has finished */
  isFinished?: boolean;

  /** Whether this is a static/past turn */
  isStatic?: boolean;

  /** Raw JSON content from tool call args */
  content?: string;

  /** Tool execution result */
  result?: ToolResult;
}

/**
 * WebFetch tool component for displaying web content fetch operations.
 *
 * Shows the URL being fetched and a content size summary on completion.
 */
export const WebFetch = React.memo(function WebFetch({
  isFinished = false,
  content,
  result,
}: WebFetchProps) {
  const { getColor } = useTheme();

  const url = useMemo(() => parseToolArg(content, 'url'), [content]);

  const title = isFinished ? 'Fetched' : 'Fetching';

  // Truncate URL for display — show hostname + path
  const displayUrl = useMemo(() => {
    if (!url) return null;
    try {
      const parsed = new URL(url);
      const path = parsed.pathname === '/' ? '' : parsed.pathname;
      const display = `${parsed.hostname}${path}`;
      return display.length > 60 ? display.slice(0, 57) + '...' : display;
    } catch {
      return url.length > 60 ? url.slice(0, 57) + '...' : url;
    }
  }, [url]);

  const target = displayUrl || 'web content';

  const summary = useMemo(() => getResultSummary(result), [result]);

  if (result?.status === 'error') {
    return (
      <Box flexDirection="column">
        <StatusInfo title={title} target={target} shimmer={false} />
        <Box marginLeft={2}>
          <Text>{getColor('error')(result.error)}</Text>
        </Box>
      </Box>
    );
  }

  return (
    <Box flexDirection="column">
      <StatusInfo title={title} target={target} shimmer={!isFinished} />
      {isFinished && summary && <Text>{getColor('secondary')(summary)}</Text>}
    </Box>
  );
});
