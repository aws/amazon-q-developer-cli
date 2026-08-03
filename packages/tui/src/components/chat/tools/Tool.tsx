import React, { useMemo } from 'react';
import { Box, Text } from './../../../renderer.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import { StatusBar } from '../status-bar/StatusBar.js';
import { useExpandableOutput } from '../../../hooks/useExpandableOutput.js';
import {
  unescapeJsonNewlines,
  unwrapResultOutput,
} from '../../../utils/tool-result.js';
import { formatToolParams } from '../../../utils/tool-params.js';
import { ToolMeta } from './ToolMeta.js';
import { ToolOutput } from './ToolOutput.js';
import { normalizeLineEndings } from '../../../utils/string.js';
import type { ToolResult } from '../../../stores/app-store.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import { MarkdownRenderer } from '../../ui/MarkdownRenderer.js';
import { useHideToolArgs } from '../../ui/HideToolArgsContext.js';
import { useToolOutputVisible } from '../../ui/VerbosityToolContext.js';
import type { StatusType } from '../../../types/componentTypes.js';
import type { ToolCallLocation } from '../../../types/agent-events.js';

const PREVIEW_LINES = 3;
// The `╰ output:` tree + green body is the in-cohort output-differentiation
// feature; off-cohort keeps mainline's bare primary lines (no header).
const PORT_ACTIVE = () => process.env.KIRO_LITE_ROLLOUT_ENABLED === '1';

export interface ToolProps {
  /** The tool name to display */
  name: string;

  /** Tool status type */
  status?: StatusType;

  /** Skip the StatusBar wrapper (use when already inside a StatusBar) */
  noStatusBar?: boolean;

  /** Whether the tool operation has finished */
  isFinished?: boolean;

  /** Whether this is a static/past turn (no expandable output) */
  isStatic?: boolean;

  /** Tool execution result containing output/error */
  result?: ToolResult;

  /** File locations associated with the tool call */
  locations?: ToolCallLocation[];

  /** Error message to display */
  errorMessage?: string | null;

  /** Raw JSON content from tool call args */
  content?: string;
}

/**
 * Generic tool component for displaying tool calls with locations and collapsible output.
 *
 * Features:
 * - Shows tool name as title (state-independent)
 * - Displays intent (__tool_use_purpose) when available
 * - Shows formatted tool params
 * - Displays file locations when provided
 * - Collapsible output with Ctrl+O expansion
 * - Error display for failed tools
 * - Static mode for past turns (no output shown)
 */
export const Tool = React.memo(function Tool({
  name,
  status,
  noStatusBar = false,
  isFinished = false,
  isStatic = false,
  result,
  locations,
  errorMessage,
  content,
}: ToolProps) {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();

  const params = useMemo(() => formatToolParams(content), [content]);

  // Format locations for display - each on its own row
  const formattedLocations = useMemo(() => {
    if (!locations || locations.length === 0) return null;

    return locations.map((loc) => {
      const fileName = loc.path.split('/').pop() || loc.path;
      return loc.line ? `${fileName}:${loc.line}` : fileName;
    });
  }, [locations]);

  // Extract and format output from result
  const { output, outputLines } = useMemo(() => {
    const { obj, text } = unwrapResultOutput(result);

    if (text)
      return {
        output: text,
        outputLines: normalizeLineEndings(text).split('\n'),
      };
    if (!obj) return { output: null, outputLines: [] };

    let outputStr: string | null = null;
    if ('text' in obj && typeof obj.text === 'string') {
      outputStr = obj.text;
    } else if ('content' in obj && typeof obj.content === 'string') {
      outputStr = obj.content;
    } else if ('result' in obj && typeof obj.result === 'string') {
      outputStr = obj.result;
    } else {
      try {
        const serialized = JSON.stringify(obj, null, 2);
        outputStr = PORT_ACTIVE()
          ? unescapeJsonNewlines(serialized)
          : serialized;
      } catch {
        // outputStr remains null
      }
    }

    const lines = outputStr ? normalizeLineEndings(outputStr).split('\n') : [];
    return { output: outputStr, outputLines: lines };
  }, [result]);

  const hasOutput = output && output.trim().length > 0;
  const outputVisible = useToolOutputVisible();

  // In-cohort output owns its wrap and cap, so this hook only serves the
  // legacy off-cohort preview.
  const { expanded, expandHint } = useExpandableOutput({
    totalItems: PORT_ACTIVE() ? 0 : outputLines.length,
    previewCount: PREVIEW_LINES,
    isStatic,
    unit: 'lines',
  });

  // In spec mode, render all tool titles via MarkdownRenderer (questions get
  // proper formatting; plain names render identically). Non-spec keeps
  // StatusInfo with its shimmer loading animation.
  const hideArgs = useHideToolArgs();
  const renderTitle = () =>
    hideArgs ? (
      <MarkdownRenderer content={name} color={getColor('primary')} />
    ) : (
      <StatusInfo title={name} shimmer={!isFinished} />
    );

  const renderMeta = () => <ToolMeta params={params} />;

  const renderLocations = () => {
    if (!formattedLocations) return null;
    return (
      <Box marginLeft={2} flexDirection="column">
        {formattedLocations.map((loc, i) => (
          <Text key={i}>{getColor('secondary')(`${glyphs.arrow} ${loc}`)}</Text>
        ))}
      </Box>
    );
  };

  const renderLegacyContent = () => {
    if (errorMessage) {
      return (
        <Box flexDirection="column">
          {renderTitle()}
          {renderMeta()}
          {renderLocations()}
          <Box marginLeft={2}>
            <Text>{getColor('error')(errorMessage)}</Text>
          </Box>
        </Box>
      );
    }
    if (isStatic || !hasOutput) {
      return (
        <Box flexDirection="column">
          {renderTitle()}
          {renderMeta()}
          {renderLocations()}
        </Box>
      );
    }
    if (expanded) {
      return (
        <Box flexDirection="column">
          {renderTitle()}
          {renderMeta()}
          {renderLocations()}
          <Box marginLeft={2} flexDirection="column">
            {outputLines.map((line, i) => (
              <Text key={i}>{getColor('primary')(line)}</Text>
            ))}
          </Box>
        </Box>
      );
    }
    return (
      <Box flexDirection="column">
        {renderTitle()}
        {renderMeta()}
        {renderLocations()}
        <Box marginLeft={2} flexDirection="column">
          {outputLines.slice(0, PREVIEW_LINES).map((line, i) => (
            <Text key={i}>{getColor('primary')(line)}</Text>
          ))}
          {expandHint && <Text>{getColor('secondary')(expandHint)}</Text>}
        </Box>
      </Box>
    );
  };

  const renderContent = () => {
    if (!PORT_ACTIVE()) return renderLegacyContent();

    if (errorMessage) {
      return (
        <Box flexDirection="column">
          {renderTitle()}
          {renderMeta()}
          {renderLocations()}
          <ToolOutput lines={errorMessage.split('\n')} isError />
        </Box>
      );
    }

    return (
      <Box flexDirection="column">
        {renderTitle()}
        {renderMeta()}
        {renderLocations()}
        {hasOutput && outputVisible && (
          <ToolOutput
            lines={outputLines}
            isStatic={isStatic}
            previewPosition="start"
          />
        )}
      </Box>
    );
  };

  if (noStatusBar) {
    return renderContent();
  }

  return <StatusBar status={status}>{renderContent()}</StatusBar>;
});
