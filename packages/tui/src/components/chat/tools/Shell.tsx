import React, { useMemo, useEffect } from 'react';
import { Box, Text } from './../../../renderer.js';
import { StatusBar, useStatusBar } from '../status-bar/StatusBar.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useExpandableOutput } from '../../../hooks/useExpandableOutput.js';
import { unwrapResultOutput } from '../../../utils/tool-result.js';
import { formatToolParams } from '../../../utils/tool-params.js';
import { ToolMeta } from './ToolMeta.js';
import { ToolOutput } from './ToolOutput.js';
import { normalizeLineEndings } from '../../../utils/string.js';
import { maxVisibleWidth } from '../../../utils/text-width.js';
import { useToolOutputVisible } from '../../ui/VerbosityToolContext.js';
import { useAppStore, type ToolResult } from '../../../stores/app-store.js';
import type { StatusType } from '../../../types/componentTypes.js';

const PREVIEW_LINES = 5;
const MAX_EXPANDED_LINES = 1000;
// The `╰ output:` tree + green body is the in-cohort output-differentiation
// feature; off-cohort keeps mainline's bare primary-colored lines (no header).
const PORT_ACTIVE = () => process.env.KIRO_LITE_ROLLOUT_ENABLED === '1';

/** Collect the first `n` lines from a chunked output buffer. Walks chunks in
 *  order, stopping as soon as we have enough. O(n) in the output count. */
function firstLines(chunks: string[][], n: number): string[] {
  if (n <= 0) return [];
  const out: string[] = [];
  for (const c of chunks) {
    for (const line of c) {
      if (out.length >= n) return out;
      out.push(line);
    }
  }
  return out;
}

/** Collect the last `n` lines from a chunked output buffer. Walks chunks in
 *  reverse, stopping as soon as we have enough. O(n) in the output count. */
function lastLines(chunks: string[][], n: number): string[] {
  if (n <= 0) return [];
  const out: string[] = [];
  for (let ci = chunks.length - 1; ci >= 0 && out.length < n; ci--) {
    const c = chunks[ci]!;
    for (let li = c.length - 1; li >= 0 && out.length < n; li--) {
      out.push(c[li]!);
    }
  }
  return out.reverse();
}

export interface ShellProps {
  /** The tool name/action */
  name: string;
  /** The bash command to display */
  command?: string;
  /** Tool call ID — used to subscribe to live output from the store */
  toolCallId?: string;
  /** Tool status type */
  status?: StatusType;
  /** Skip the StatusBar wrapper (use when already inside a StatusBar) */
  noStatusBar?: boolean;
  /** Whether the command has finished executing */
  isFinished?: boolean;
  /** Whether this is a static/past turn (no expandable output) */
  isStatic?: boolean;
  /** Tool execution result containing output/error */
  result?: ToolResult;
  /** Raw JSON content from tool call args */
  content?: string;
}

/**
 * Shell tool component for displaying bash command execution.
 */
export const Shell = React.memo(function Shell({
  name,
  command,
  toolCallId,
  status,
  noStatusBar = false,
  isFinished = false,
  isStatic = false,
  result,
  content,
}: ShellProps) {
  // Subscribe to live output directly from the store — only this Shell
  // re-renders when new output arrives, not the entire ConversationView.
  const liveOutput = useAppStore((s) =>
    toolCallId ? s.liveOutputs.get(toolCallId) : undefined
  );
  const { getColor } = useTheme();

  const params = useMemo(
    () => formatToolParams(content, ['command']),
    [content]
  );

  let statusBarContext: ReturnType<typeof useStatusBar> | null = null;
  try {
    statusBarContext = useStatusBar();
  } catch {
    // Not within a StatusBar context
  }

  const { setStatus } = statusBarContext ?? {
    setStatus: () => {},
  };

  const displayCommand = command || undefined;

  const isTimeoutError = useMemo(() => {
    if (result?.status === 'error') {
      const errorMsg = result.error.toLowerCase();
      return errorMsg.includes('timeout') || errorMsg.includes('timed out');
    }
    return false;
  }, [result]);

  const errorMessage = useMemo(() => {
    if (result?.status === 'error') {
      return isTimeoutError ? 'Command timed out' : result.error;
    }
    return null;
  }, [result, isTimeoutError]);

  // Unify output source: use result when available, otherwise liveOutput during execution.
  // Output is represented as `string[][]` (chunks of lines) so that per-flush append is
  // O(chunks) rather than O(total_lines) — the outer array holds chunk references, which
  // we can spread cheaply even when total line count is in the tens of thousands.
  const { outputChunks, exitCode } = useMemo((): {
    outputChunks: string[][];
    exitCode: number | null;
  } => {
    if (result) {
      const { obj, text } = unwrapResultOutput(result);

      if (text) {
        const lines = normalizeLineEndings(text).split('\n');
        return {
          outputChunks: lines.length > 0 ? [lines] : [],
          exitCode: null,
        };
      }
      if (!obj) return { outputChunks: [], exitCode: null };

      let code: number | null = null;
      if ('exit_status' in obj) {
        const exitStatus = obj.exit_status;
        if (typeof exitStatus === 'number') {
          code = exitStatus;
        } else if (typeof exitStatus === 'string') {
          const match = exitStatus.match(/(\d+)/);
          if (match && match[1]) {
            code = parseInt(match[1], 10);
          }
        }
      } else if ('exitCode' in obj && typeof obj.exitCode === 'number') {
        code = obj.exitCode;
      } else if ('code' in obj && typeof obj.code === 'number') {
        code = obj.code;
      }

      let outputStr: string | null = null;
      if (
        'stdout' in obj &&
        typeof obj.stdout === 'string' &&
        obj.stdout.trim()
      ) {
        outputStr = obj.stdout;
      } else if (
        'output' in obj &&
        typeof obj.output === 'string' &&
        obj.output.trim()
      ) {
        outputStr = obj.output;
      } else if (
        'stderr' in obj &&
        typeof obj.stderr === 'string' &&
        obj.stderr.trim()
      ) {
        outputStr = obj.stderr;
      }

      if (!outputStr) return { outputChunks: [], exitCode: code };
      const lines = normalizeLineEndings(outputStr).split('\n');
      return {
        outputChunks: lines.length > 0 ? [lines] : [],
        exitCode: code,
      };
    }

    if (liveOutput && liveOutput.length > 0) {
      return { outputChunks: liveOutput, exitCode: null };
    }

    return { outputChunks: [], exitCode: null };
  }, [result, liveOutput]);

  const { totalLines, maxOutputWidth } = useMemo(() => {
    let count = 0;
    let width = 0;
    for (const chunk of outputChunks) {
      count += chunk.length;
      width = Math.max(width, maxVisibleWidth(chunk));
    }
    return { totalLines: count, maxOutputWidth: width };
  }, [outputChunks]);

  const hasOutput = totalLines > 0;
  const outputVisible = useToolOutputVisible();

  const {
    expanded,
    expandHint,
    hiddenCount,
    effectivePreviewCount,
    outputMaxChars,
    persistOutput,
  } = useExpandableOutput({
    totalItems: !PORT_ACTIVE() || outputVisible ? totalLines : 0,
    previewCount: PREVIEW_LINES,
    maxContentWidth: !PORT_ACTIVE() || outputVisible ? maxOutputWidth : 0,
    isStatic,
    unit: 'lines',
    applyVerbosityOutputCap: PORT_ACTIVE(),
  });

  useEffect(() => {
    if (isFinished && exitCode !== null && exitCode !== 0) {
      setStatus('error');
    } else if (isTimeoutError) {
      setStatus('error');
    }
  }, [isFinished, exitCode, isTimeoutError, setStatus]);

  if (!PORT_ACTIVE()) {
    const legacy = renderLegacyShell({
      name,
      displayCommand,
      params,
      isFinished,
      isStatic,
      result,
      liveOutput,
      hasOutput,
      errorMessage,
      outputChunks,
      totalLines,
      expanded,
      expandHint,
      hiddenCount,
      getColor,
    });
    if (noStatusBar) return legacy;
    return <StatusBar status={status}>{legacy}</StatusBar>;
  }

  // Simple mode: no output yet
  if (!result && !liveOutput) {
    const simpleContent = (
      <Box flexDirection="column">
        <StatusInfo
          title={name}
          target={displayCommand}
          shimmer={!isFinished}
        />
        <ToolMeta params={params} />
      </Box>
    );
    if (noStatusBar) return simpleContent;
    return <StatusBar status={status}>{simpleContent}</StatusBar>;
  }

  if ((isStatic && !persistOutput) || (!hasOutput && !errorMessage)) {
    return (
      <Box flexDirection="column">
        <StatusInfo
          title={name}
          target={displayCommand}
          shimmer={!isFinished}
        />
        <ToolMeta params={params} />
      </Box>
    );
  }

  // Error
  if (errorMessage) {
    return (
      <Box flexDirection="column">
        <StatusInfo
          title={name}
          target={displayCommand}
          shimmer={!isFinished}
        />
        <ToolMeta params={params} />
        <ToolOutput lines={errorMessage.split('\n')} isError />
      </Box>
    );
  }

  if (!outputVisible) {
    return (
      <Box flexDirection="column">
        <StatusInfo
          title={name}
          target={displayCommand}
          shimmer={!isFinished}
        />
        <ToolMeta params={params} />
      </Box>
    );
  }

  if (expanded) {
    return (
      <Box flexDirection="column">
        <StatusInfo
          title={name}
          target={displayCommand}
          shimmer={!isFinished}
        />
        <ToolMeta params={params} />
        <ToolOutput
          lines={firstLines(outputChunks, totalLines)}
          maxChars={outputMaxChars}
        />
      </Box>
    );
  }

  const previewLines = isFinished
    ? firstLines(outputChunks, effectivePreviewCount)
    : lastLines(outputChunks, effectivePreviewCount);

  const hint = isFinished
    ? expandHint
    : hiddenCount > 0
      ? `...+${hiddenCount} lines above (ctrl+o to toggle)`
      : expandHint || undefined;

  return (
    <Box flexDirection="column">
      <StatusInfo title={name} target={displayCommand} shimmer={!isFinished} />
      <ToolMeta params={params} />
      <ToolOutput
        lines={previewLines}
        maxChars={outputMaxChars}
        expandHint={hint}
      />
    </Box>
  );
});

function renderLegacyShell({
  name,
  displayCommand,
  params,
  isFinished,
  isStatic,
  result,
  liveOutput,
  hasOutput,
  errorMessage,
  outputChunks,
  totalLines,
  expanded,
  expandHint,
  hiddenCount,
  getColor,
}: {
  name: string;
  displayCommand?: string;
  params: string[] | null;
  isFinished: boolean;
  isStatic: boolean;
  result?: ToolResult;
  liveOutput?: string[][] | null;
  hasOutput: boolean;
  errorMessage: string | null;
  outputChunks: string[][];
  totalLines: number;
  expanded: boolean;
  expandHint: string;
  hiddenCount: number;
  getColor: (name: string) => (s: string) => string;
}) {
  const head = (
    <>
      <StatusInfo title={name} target={displayCommand} shimmer={!isFinished} />
      <ToolMeta params={params} />
    </>
  );
  if ((!result && !liveOutput) || isStatic || (!hasOutput && !errorMessage)) {
    return <Box flexDirection="column">{head}</Box>;
  }
  if (errorMessage) {
    return (
      <Box flexDirection="column">
        {head}
        <Box marginLeft={2}>
          <Text>{getColor('error')(errorMessage)}</Text>
        </Box>
      </Box>
    );
  }
  if (expanded) {
    const expandedLines = firstLines(outputChunks, MAX_EXPANDED_LINES);
    const truncated = totalLines > MAX_EXPANDED_LINES;
    return (
      <Box flexDirection="column">
        {head}
        <Box marginLeft={2} flexDirection="column">
          {expandedLines.map((line, i) => (
            <Text key={i}>{getColor('primary')(line)}</Text>
          ))}
          {truncated && (
            <Text>
              {getColor('secondary')(
                `[truncated, showing ${MAX_EXPANDED_LINES} of ${totalLines} lines]`
              )}
            </Text>
          )}
        </Box>
      </Box>
    );
  }
  const previewLines = isFinished
    ? firstLines(outputChunks, PREVIEW_LINES)
    : lastLines(outputChunks, PREVIEW_LINES);
  return (
    <Box flexDirection="column">
      {head}
      <Box marginLeft={2} flexDirection="column">
        {!isFinished && hiddenCount > 0 && (
          <Text>
            {getColor('secondary')(
              `...+${hiddenCount} lines above (ctrl+o to toggle)`
            )}
          </Text>
        )}
        {previewLines.map((line, i) => (
          <Text key={i}>{getColor('primary')(line)}</Text>
        ))}
        {isFinished && expandHint && (
          <Text>{getColor('secondary')(expandHint)}</Text>
        )}
      </Box>
    </Box>
  );
}
