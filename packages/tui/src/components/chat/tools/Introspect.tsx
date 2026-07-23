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
import { useExpandableOutput } from '../../../hooks/useExpandableOutput.js';
import { useToolOutputVisible } from '../../ui/VerbosityToolContext.js';
import { normalizeLineEndings } from '../../../utils/string.js';
import { maxVisibleWidth } from '../../../utils/text-width.js';
import type { ToolResult } from '../../../stores/app-store.js';

const PREVIEW_LINES = 5;

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

  const {
    expanded,
    expandHint,
    effectivePreviewCount,
    outputMaxChars,
    persistOutput,
  } = useExpandableOutput({
    totalItems: portEnabled && outputVisible ? docLines.length : 0,
    previewCount: PREVIEW_LINES,
    maxContentWidth:
      portEnabled && outputVisible ? maxVisibleWidth(docLines) : 0,
    isStatic,
    unit: 'lines',
    applyVerbosityOutputCap: true,
  });

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

  // Header + args only when: off-rollout, output filtered off, no doc body, or
  // a static past turn with persistOutput off (collapse to keep history compact
  // — parity with Shell/Grep/Glob).
  const showDoc =
    portEnabled &&
    outputVisible &&
    docLines.length > 0 &&
    !(isStatic && !persistOutput);

  if (!showDoc) {
    return (
      <Box flexDirection="column">
        <StatusInfo title={title} target={target} shimmer={!isFinished} />
        <ToolMeta params={params} />
      </Box>
    );
  }

  const visible = expanded
    ? docLines
    : docLines.slice(0, effectivePreviewCount);

  return (
    <Box flexDirection="column">
      <StatusInfo title={title} target={target} shimmer={!isFinished} />
      <ToolMeta params={params} />
      <ToolOutput
        lines={visible}
        maxChars={outputMaxChars}
        expandHint={!expanded ? expandHint : undefined}
      />
    </Box>
  );
});
