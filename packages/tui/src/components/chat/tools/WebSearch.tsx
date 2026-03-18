import React, { useMemo } from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import { parseToolArg, getResultSummary } from '../../../utils/tool-result.js';
import { formatToolParams } from '../../../utils/tool-params.js';
import { ToolMeta } from './ToolMeta.js';
import type { ToolResult } from '../../../stores/app-store.js';
import { getToolLabel } from '../../../types/tool-status.js';
export interface WebSearchProps {
  /** Whether the search has finished */
  isFinished?: boolean;

  /** Whether this is a static/past turn */
  isStatic?: boolean;

  /** Raw JSON content from tool call args */
  content?: string;

  /** Tool execution result */
  result?: ToolResult;
}

/**
 * WebSearch tool component for displaying web search operations.
 *
 * Shows the search query and a brief result summary on completion.
 */
export const WebSearch = React.memo(function WebSearch({
  isFinished = false,
  content,
  result,
}: WebSearchProps) {
  const { getColor } = useTheme();

  const query = useMemo(() => parseToolArg(content, 'query'), [content]);

  const params = useMemo(() => formatToolParams(content, ['query']), [content]);

  const title = getToolLabel('web_search');
  const target = query ? `"${query}"` : 'the web';

  const summary = useMemo(() => getResultSummary(result), [result]);

  if (result?.status === 'error') {
    return (
      <Box flexDirection="column">
        <StatusInfo title={title} target={target} shimmer={false} />
        <ToolMeta params={params} />
        <Box marginLeft={2}>
          <Text>{getColor('error')(result.error)}</Text>
        </Box>
      </Box>
    );
  }

  return (
    <Box flexDirection="column">
      <StatusInfo title={title} target={target} shimmer={!isFinished} />
      <ToolMeta params={params} />
      {isFinished && summary && <Text>{getColor('secondary')(summary)}</Text>}
    </Box>
  );
});
