import React, { useMemo } from 'react';
import { Box } from 'ink';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import { parseToolArg } from '../../../utils/tool-result.js';
import { getToolLabel } from '../../../types/tool-status.js';
import type { ToolResult } from '../../../stores/app-store.js';

export interface IntrospectProps {
  /** Whether the introspection has finished */
  isFinished?: boolean;

  /** Whether this is a static/past turn */
  isStatic?: boolean;

  /** Raw JSON content from tool call args */
  content?: string;

  /** Tool execution result */
  result?: ToolResult;
}

/**
 * Introspect tool component for displaying self-documentation lookups.
 *
 * Shows the query or doc_path being looked up. Result output is hidden
 * since it contains internal documentation meant for the LLM, not the user.
 */
export const Introspect = React.memo(function Introspect({
  isFinished = false,
  content,
  result,
}: IntrospectProps) {
  const { getColor } = useTheme();

  const target = useMemo(() => {
    const query = parseToolArg(content, 'query');
    if (query) return query;
    const docPath = parseToolArg(content, 'doc_path');
    if (docPath) return docPath;
    return '';
  }, [content]);

  const title = getToolLabel('introspect', isFinished);

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

  return <StatusInfo title={title} target={target} shimmer={!isFinished} />;
});
