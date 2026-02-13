import React, { useMemo } from 'react';
import { Box, Text } from 'ink';
import { StatusBar } from '../status-bar/StatusBar.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { parseToolArg, unwrapResultOutput } from '../../../utils/tool-result.js';
import type { ToolResult } from '../../../stores/app-store.js';
import type { StatusType } from '../../../types/componentTypes.js';

const PREVIEW_LINES = 3;

/** Operation labels for display */
const OP_LABELS: Record<string, [string, string]> = {
  search_symbols: ['Searching symbols', 'Searched symbols'],
  lookup_symbols: ['Looking up symbols', 'Looked up symbols'],
  find_references: ['Finding references', 'Found references'],
  goto_definition: ['Going to definition', 'Went to definition'],
  get_document_symbols: ['Getting symbols', 'Got symbols'],
  get_diagnostics: ['Getting diagnostics', 'Got diagnostics'],
  get_hover: ['Getting hover info', 'Got hover info'],
  get_completions: ['Getting completions', 'Got completions'],
  pattern_search: ['Pattern searching', 'Pattern searched'],
  pattern_rewrite: ['Pattern rewriting', 'Pattern rewrote'],
  generate_codebase_overview: ['Generating overview', 'Generated overview'],
  search_codebase_map: ['Exploring codebase', 'Explored codebase'],
  rename_symbol: ['Renaming symbol', 'Renamed symbol'],
  format: ['Formatting', 'Formatted'],
};

const VERBOSE_OPS = new Set([
  'generate_codebase_overview',
  'search_codebase_map',
]);

export interface CodeProps {
  name?: string;
  status?: StatusType;
  noStatusBar?: boolean;
  isFinished?: boolean;
  isStatic?: boolean;
  content?: string;
  result?: ToolResult;
}

export const Code = React.memo(function Code({
  name,
  status,
  noStatusBar = false,
  isFinished = false,
  isStatic = false,
  content,
  result,
}: CodeProps) {
  const { getColor } = useTheme();

  const operation = useMemo(() => parseToolArg(content, 'operation'), [content]);
  const symbolName = useMemo(() => parseToolArg(content, 'symbol_name'), [content]);
  const filePath = useMemo(() => parseToolArg(content, 'file_path'), [content]);
  const pattern = useMemo(() => parseToolArg(content, 'pattern'), [content]);

  const labels = operation ? OP_LABELS[operation] : null;
  const title = isFinished
    ? (labels?.[1] ?? 'Used code')
    : (labels?.[0] ?? name ?? 'Using code');

  // Build a concise target string
  const target = useMemo(() => {
    if (symbolName) return symbolName;
    if (pattern) return `"${pattern}"`;
    if (filePath) return filePath.split('/').pop() || filePath;
    return operation || undefined;
  }, [symbolName, pattern, filePath, operation]);

  // Parse result into summary lines
  const summaryLines = useMemo((): string[] => {
    const { obj, text } = unwrapResultOutput(result);
    if (!obj && !text) return [];

    // Text results: take first few lines
    if (text) {
      return text.split('\n').filter(l => l.trim()).slice(0, PREVIEW_LINES);
    }

    // For structured results, extract a brief summary
    if (!obj) return [];

    // search_symbols / lookup_symbols — show matched symbol names
    if (Array.isArray(obj.symbols)) {
      return (obj.symbols as any[]).slice(0, PREVIEW_LINES).map(
        s => `→ ${s.name || s}`,
      );
    }

    // find_references — show count
    if (Array.isArray(obj.references)) {
      return [`${(obj.references as any[]).length} references`];
    }

    // get_document_symbols
    if (Array.isArray(obj.documentSymbols)) {
      return (obj.documentSymbols as any[]).slice(0, PREVIEW_LINES).map(
        s => `→ ${s.name || s}`,
      );
    }

    // Fallback: try text fields
    if (typeof obj.text === 'string') {
      return (obj.text as string).split('\n').filter(l => l.trim()).slice(0, PREVIEW_LINES);
    }

    return [];
  }, [result]);

  const renderContent = () => {
    if (result?.status === 'error') {
      return (
        <Box flexDirection="column">
          <StatusInfo title={title} target={target} shimmer={!isFinished} />
          <Box marginLeft={2}>
            <Text>{getColor('error')(result.error)}</Text>
          </Box>
        </Box>
      );
    }

    if (!isFinished || summaryLines.length === 0 || isStatic || (operation && VERBOSE_OPS.has(operation))) {
      return <StatusInfo title={title} target={target} shimmer={!isFinished} />;
    }

    return (
      <Box flexDirection="column">
        <StatusInfo title={title} target={target} shimmer={!isFinished} />
        {summaryLines.map((line, i) => (
          <Box key={i} marginLeft={2}>
            <Text>{getColor('secondary')(line)}</Text>
          </Box>
        ))}
      </Box>
    );
  };

  if (noStatusBar) {
    return renderContent();
  }

  return <StatusBar status={status}>{renderContent()}</StatusBar>;
});
