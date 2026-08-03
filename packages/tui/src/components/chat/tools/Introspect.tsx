import React, { useMemo } from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import {
  parseToolArg,
  unwrapResultOutput,
} from '../../../utils/tool-result.js';
import { formatToolParams } from '../../../utils/tool-params.js';
import { ToolMeta } from './ToolMeta.js';
import { ToolOutput } from './ToolOutput.js';
import { getToolLabel } from '../../../types/tool-status.js';
import { useToolOutputVisible } from '../../ui/VerbosityToolContext.js';
import { normalizeLineEndings } from '../../../utils/string.js';
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

/** Pull the human-readable documentation body out of the introspect result.
 *  The tool returns `{ documentation, query_context }` (or plain text on some
 *  engines); prefer the documentation field, else the text envelope. */
function extractDocumentation(result: ToolResult | undefined): string | null {
  const { obj, text } = unwrapResultOutput(result);
  if (obj && typeof obj.documentation === 'string') return obj.documentation;
  if (text) return text;
  if (obj && typeof obj.text === 'string') return obj.text;
  return null;
}

/**
 * Introspect tool component for displaying self-documentation lookups.
 *
 * Shows the query/doc_path being looked up plus the returned documentation
 * body (parity with lite, which surfaces it under `output:`), gated by the
 * verbosity output filter and bounded by outputMaxLines/outputMaxChars. ctrl+o
 * expands to the full doc.
 */
export const Introspect = React.memo(function Introspect({
  isFinished = false,
  isStatic = false,
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

  const params = useMemo(
    () => formatToolParams(content, ['query', 'doc_path']),
    [content]
  );

  const docLines = useMemo(() => {
    const doc = extractDocumentation(result);
    if (!doc) return [];
    return normalizeLineEndings(doc).split('\n');
  }, [result]);
  const outputVisible = useToolOutputVisible();
  const portEnabled = process.env.KIRO_LITE_ROLLOUT_ENABLED === '1';

  const title = getToolLabel('introspect');

  if (result?.status === 'error') {
    return (
      <Box flexDirection="column">
        <StatusInfo title={title} target={target} shimmer={false} />
        <ToolMeta params={params} />
        {portEnabled ? (
          <ToolOutput lines={result.error.split('\n')} isError />
        ) : (
          <Box marginLeft={2}>
            <Text>{getColor('error')(result.error)}</Text>
          </Box>
        )}
      </Box>
    );
  }

  const showDoc = portEnabled && outputVisible && docLines.length > 0;

  if (!showDoc) {
    return (
      <Box flexDirection="column">
        <StatusInfo title={title} target={target} shimmer={!isFinished} />
        <ToolMeta params={params} />
      </Box>
    );
  }

  return (
    <Box flexDirection="column">
      <StatusInfo title={title} target={target} shimmer={!isFinished} />
      <ToolMeta params={params} />
      <ToolOutput
        lines={docLines}
        isStatic={isStatic}
        previewPosition="start"
      />
    </Box>
  );
});
