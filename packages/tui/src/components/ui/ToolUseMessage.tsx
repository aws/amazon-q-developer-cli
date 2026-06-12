import React, { useEffect, useMemo } from 'react';
import { Box, Text as InkText } from './../../renderer.js';
import { StatusBar, useStatusBar } from '../chat/status-bar/StatusBar.js';
import { StatusInfo } from './status/StatusInfo.js';
import type { StatusType } from '../../types/componentTypes.js';
import { Text } from './text/Text.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { Write } from '../chat/tools/Write.js';
import { Read } from '../chat/tools/Read.js';
import { Shell } from '../chat/tools/Shell.js';
import { Grep } from '../chat/tools/Grep.js';
import { Glob } from '../chat/tools/Glob.js';
import { Ls } from '../chat/tools/Ls.js';
import { Code } from '../chat/tools/Code.js';
import { Introspect } from '../chat/tools/Introspect.js';
import { ImageRead } from '../chat/tools/ImageRead.js';
import { WebSearch } from '../chat/tools/WebSearch.js';
import { WebFetch } from '../chat/tools/WebFetch.js';
import { SessionTool } from '../chat/tools/SessionTool.js';
import { Tool } from '../chat/tools/Tool.js';
import { ToolMeta } from '../chat/tools/ToolMeta.js';
import { formatToolParams } from '../../utils/tool-params.js';
import { parseToolArg } from '../../utils/tool-result.js';
import { ToolUseStatus, type ToolResult } from '../../stores/app-store.js';
import {
  WRITE_TOOL_NAMES,
  READ_TOOL_NAMES,
  SHELL_TOOL_NAMES,
  WEB_SEARCH_TOOL_NAMES,
  WEB_FETCH_TOOL_NAMES,
  GREP_TOOL_NAMES,
  GLOB_TOOL_NAMES,
  LS_TOOL_NAMES,
  CODE_TOOL_NAMES,
  SESSION_TOOL_NAMES,
  resolveToolId,
  kindToToolId,
  INTROSPECT_TOOL_NAMES,
  IMAGE_READ_TOOL_NAMES,
  TASK_TOOL_NAMES,
  KNOWLEDGE_TOOL_NAMES,
  type ToolDiff,
  type ToolKind,
  type ToolCallLocation,
} from '../../types/agent-events.js';
import { getToolLabel } from '../../types/tool-status.js';
import { useKeybindings } from '../../hooks/useKeybindings.js';
import { useGlyphs } from '../../hooks/useGlyphs.js';
import { useExpandableOutput } from '../../hooks/useExpandableOutput.js';
import { useHideToolArgs } from './HideToolArgsContext.js';
import {
  collapsedToolPreview,
  isCollapsibleTool,
} from '../../utils/collapsed-tool-view.js';

export interface ToolUseMessageProps {
  id: string;
  name: string;
  content: string;
  diff?: ToolDiff;
  isFinished?: boolean;
  status?: ToolUseStatus;
  result?: ToolResult;
  kind?: ToolKind;
  locations?: ToolCallLocation[];
  barColor?: string;
  isStatic?: boolean;
  /** If set, shows a colored agent name prefix (for subagent tool calls) */
  agentLabel?: string;
  agentLabelColor?: string;
}

export const ToolUseMessage = React.memo<ToolUseMessageProps>(
  function ToolUseMessage({
    id,
    name,
    content,
    diff,
    isFinished = false,
    status,
    result,
    kind,
    locations,
    barColor,
    isStatic = false,
    agentLabel,
    agentLabelColor,
  }) {
    const { getColor, wrapDisabled } = useTheme();
    const glyphs = useGlyphs();
    const keybindings = useKeybindings();
    // Map tool status to StatusBar status icon
    const statusIcon: StatusType | undefined = useMemo(() => {
      if (status === ToolUseStatus.Rejected) return 'error';
      if (result?.status === 'cancelled') return 'error';
      if (result?.status === 'error') return 'error';
      if (status === ToolUseStatus.Approved && isFinished) return 'success';
      if (status === ToolUseStatus.Pending) return 'paused';
      if (isFinished) return 'success';
      return 'executing';
    }, [status, isFinished, result]);

    const showEscHint = statusIcon === 'executing' && !isStatic;

    // Under wrapDisabled, drop the StatusBar chrome (vertical colored bar +
    // margin) entirely, both in live and static contexts. This keeps layout
    // identical across live/static transitions and produces clean copy-paste
    // output with no leading whitespace.
    const skipStatusBar = wrapDisabled;

    const inner = (
      <>
        {agentLabel && (
          <Box>
            <InkText color={agentLabelColor ?? 'gray'} dimColor>
              {agentLabel}
            </InkText>
          </Box>
        )}
        <ToolUseContent
          id={id}
          name={name}
          kind={kind}
          content={content}
          diff={diff}
          isFinished={isFinished}
          status={status}
          result={result}
          isStatic={isStatic}
          locations={locations}
        />
        {showEscHint && (
          <Text>
            {getColor('muted')(
              `${keybindings.label('cancelStream')} to cancel`
            )}
          </Text>
        )}
      </>
    );

    if (skipStatusBar) return inner;

    return (
      <StatusBar status={statusIcon} barColor={barColor}>
        {inner}
      </StatusBar>
    );
  }
);

interface ToolContentProps {
  id: string;
  name: string;
  kind?: ToolKind;
  content: string;
  diff?: ToolDiff;
  isFinished: boolean;
  status?: ToolUseStatus;
  result?: ToolResult;
  isStatic: boolean;
  locations?: ToolCallLocation[];
}

/**
 * Inner component — routes between the collapsed (spec mode) and full tool
 * renders. Spec mode hides verbose args/diff/command/output by default to keep
 * the conversation uncluttered; Ctrl+O expands to the full render. Every other
 * mode defaults to the full render, byte-identical to before.
 */
const ToolUseContent = React.memo(function ToolUseContent(
  props: ToolContentProps
) {
  const hideArgs = useHideToolArgs();
  if (hideArgs && isCollapsibleTool(props.content)) {
    return <CollapsedToolEntry {...props} />;
  }
  return <FullToolContent {...props} />;
});

/**
 * Spec-mode collapsed render: a title + the first line of the primary arg
 * (e.g. a spawned subagent's prompt), with a "ctrl+o to expand" affordance.
 * Expansion reuses the global `toolOutputsExpanded` flag (shared with tool
 * output expansion), so a single Ctrl+O reveals the full render below.
 */
const CollapsedToolEntry = React.memo(function CollapsedToolEntry(
  props: ToolContentProps
) {
  const { name, kind, content, isStatic } = props;
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  // Register this entry as Ctrl+O-expandable and read the shared expanded flag.
  const { expanded } = useExpandableOutput({
    totalItems: 2,
    previewCount: 1,
    isStatic,
  });

  if (expanded) return <FullToolContent {...props} />;

  const { title, target, preview } = collapsedToolPreview(name, kind, content);
  const muted = getColor('muted');
  const hint = getColor('secondary');
  const showMeta = !!preview || !isStatic;

  return (
    <Box flexDirection="column">
      <StatusInfo title={title} target={target} />
      {showMeta && (
        <Box marginLeft={2}>
          <Text wrap="wrap">
            {muted(`${glyphs.cornerBottomLeftRound} `)}
            {preview && muted(preview)}
            {!isStatic && hint(`${preview ? ' ' : ''}(ctrl+o to expand)`)}
          </Text>
        </Box>
      )}
    </Box>
  );
});

/** Full tool render — lives inside StatusBar to access requestRemeasure */
const FullToolContent = React.memo(function FullToolContent({
  id,
  name,
  kind,
  content,
  diff,
  isFinished,
  status,
  result,
  isStatic,
  locations,
}: ToolContentProps) {
  const { requestRemeasure } = useStatusBar();

  // A tool is only visually complete if it's finished AND no longer pending approval
  const effectiveFinished = isFinished && status !== ToolUseStatus.Pending;

  // Remeasure when status or isFinished changes — these change the rendered content height
  useEffect(() => {
    requestRemeasure();
  }, [status, isFinished, requestRemeasure]);

  if (status === ToolUseStatus.Rejected || result?.status === 'cancelled') {
    try {
      const parsed = JSON.parse(content);
      const path = parsed.path || parsed.command || 'file';
      const label = result?.status === 'cancelled' ? 'Cancelled' : 'Rejected';
      return <StatusInfo title={label} target={path} />;
    } catch {
      const label = result?.status === 'cancelled' ? 'Cancelled' : 'Rejected';
      return <StatusInfo title={label} target={name} />;
    }
  }

  // Write/Read/ImageRead/Task components don't accept a `result` prop and
  // therefore can't render errors themselves. Route failed calls for those
  // tools through FallbackError so the user still sees the error and the
  // attempted arguments. All other tool components handle errors inline.
  if (result?.status === 'error' && effectiveFinished) {
    const toolRendersOwnError =
      !WRITE_TOOL_NAMES.has(name) &&
      kind !== 'edit' &&
      !READ_TOOL_NAMES.has(name) &&
      kind !== 'read' &&
      !IMAGE_READ_TOOL_NAMES.has(name) &&
      !TASK_TOOL_NAMES.has(name);
    if (!toolRendersOwnError) {
      // Resolve a friendly label by name, falling back to kind — KAS sends
      // wire names (e.g. "read_files") that aren't in the builtin name sets,
      // but routing already keyed on kind, so reuse it for the label too.
      const toolId = resolveToolId(name) ?? kindToToolId(kind);
      const displayName = toolId ? getToolLabel(toolId) : name;
      return (
        <FallbackError
          name={displayName}
          content={content}
          error={result.error}
        />
      );
    }
  }

  if (WRITE_TOOL_NAMES.has(name) || kind === 'edit') {
    // Extract start line from locations for accurate diff line numbers
    const startLine = locations?.[0]?.line;
    return (
      <Write
        oldText={diff?.oldText}
        newText={diff?.newText}
        filePath={diff?.path}
        content={content}
        isFinished={effectiveFinished}
        isStatic={isStatic}
        startLine={startLine}
      />
    );
  }

  if (READ_TOOL_NAMES.has(name) || kind === 'read') {
    return (
      <Read
        noStatusBar
        isFinished={effectiveFinished}
        isStatic={isStatic}
        content={content}
      />
    );
  }

  if (SHELL_TOOL_NAMES.has(name)) {
    const title = getToolLabel('shell');
    let command: string | undefined;
    try {
      const parsed = JSON.parse(content);
      command = parsed.command;
    } catch {
      /* ignore */
    }
    return (
      <Shell
        name={title}
        command={command}
        toolCallId={id}
        noStatusBar
        isFinished={effectiveFinished}
        isStatic={isStatic}
        result={result}
        content={content}
      />
    );
  }

  if (WEB_SEARCH_TOOL_NAMES.has(name)) {
    return (
      <WebSearch
        isFinished={effectiveFinished}
        isStatic={isStatic}
        content={content}
        result={result}
      />
    );
  }

  if (WEB_FETCH_TOOL_NAMES.has(name)) {
    return (
      <WebFetch
        isFinished={effectiveFinished}
        isStatic={isStatic}
        content={content}
        result={result}
      />
    );
  }

  if (GREP_TOOL_NAMES.has(name)) {
    return (
      <Grep
        noStatusBar
        isFinished={effectiveFinished}
        isStatic={isStatic}
        content={content}
        result={result}
      />
    );
  }

  if (GLOB_TOOL_NAMES.has(name)) {
    return (
      <Glob
        noStatusBar
        isFinished={effectiveFinished}
        isStatic={isStatic}
        content={content}
        result={result}
      />
    );
  }

  // TODO: Remove Ls and ImageRead branches once legacy tool names are cleaned up.
  // These only render for old saved conversations that had separate ls/imageRead tool calls.
  if (LS_TOOL_NAMES.has(name)) {
    return (
      <Ls
        noStatusBar
        isFinished={effectiveFinished}
        isStatic={isStatic}
        content={content}
        result={result}
      />
    );
  }

  if (CODE_TOOL_NAMES.has(name)) {
    return (
      <Code
        noStatusBar
        isFinished={effectiveFinished}
        isStatic={isStatic}
        content={content}
        result={result}
      />
    );
  }

  if (SESSION_TOOL_NAMES.has(name)) {
    return (
      <SessionTool
        name={name}
        isFinished={effectiveFinished}
        isStatic={isStatic}
        content={content}
        result={result}
      />
    );
  }

  if (INTROSPECT_TOOL_NAMES.has(name)) {
    return (
      <Introspect
        isFinished={effectiveFinished}
        isStatic={isStatic}
        content={content}
        result={result}
      />
    );
  }

  if (IMAGE_READ_TOOL_NAMES.has(name)) {
    return (
      <ImageRead
        noStatusBar
        isFinished={effectiveFinished}
        isStatic={isStatic}
        content={content}
      />
    );
  }

  // Goal tool — compact one-liner showing command result
  if (name === 'goal') {
    const labels: Record<string, string> = {
      complete: '✓ Goal complete',
      status: 'Goal status',
    };
    let label = 'Goal';
    let detail: string | null = null;
    try {
      const parsed = JSON.parse(content);
      if (parsed.command && labels[parsed.command]) {
        label = labels[parsed.command]!;
      }
      // Show relevant detail per action
      if (parsed.summary) detail = parsed.summary;
      else if (parsed.description) detail = parsed.description;
    } catch {
      /* ignore */
    }
    return (
      <>
        <StatusInfo title={label} />
        {detail && <ToolMeta params={[detail]} />}
      </>
    );
  }

  // Task tool — show a compact one-liner since the Activity Tray surfaces task state
  if (TASK_TOOL_NAMES.has(name)) {
    const labels: Record<string, string> = {
      create: 'Task list created',
      complete: 'Tasks updated',
      add: 'Tasks added',
      remove: 'Tasks removed',
      list: 'Tasks listed',
    };
    let label = 'Task';
    try {
      const parsed = JSON.parse(content);
      if (parsed.command && labels[parsed.command]) {
        label = labels[parsed.command]!;
      }
    } catch {
      /* ignore */
    }
    return <StatusInfo title={label} />;
  }

  // Knowledge tool — show "Knowledge <command>" + remaining args, same pattern
  // as Grep (primary arg as target, the rest via ToolMeta). The rich
  // KnowledgePanel surfaces full knowledge-base state separately.
  if (KNOWLEDGE_TOOL_NAMES.has(name)) {
    const title = getToolLabel('knowledge');
    const command = parseToolArg(content, 'command');
    const params = formatToolParams(content, ['command']);
    return (
      <>
        <StatusInfo title={title} target={command || undefined} />
        <ToolMeta params={params} />
      </>
    );
  }

  // Fallback: use generic Tool component
  // For unrecognized tools that failed, show a one-liner matching Rejected/Cancelled pattern
  if (result?.status === 'error' && effectiveFinished) {
    return <FallbackError name={name} content={content} error={result.error} />;
  }

  const toolId = resolveToolId(name);
  const fallbackName = toolId ? getToolLabel(toolId) : name;
  return (
    <Tool
      name={fallbackName}
      noStatusBar
      isFinished={effectiveFinished}
      isStatic={isStatic}
      result={result}
      locations={locations}
      content={content}
    />
  );
});

/** Fallback renderer for failed tool calls. Shows the tool name, a target
 *  extracted from the args (path/command/pattern/etc), the remaining args
 *  as meta, and the error message. */
const FallbackError = React.memo(function FallbackError({
  name,
  content,
  error,
}: {
  name: string;
  content: string;
  error: string;
}) {
  const { getColor } = useTheme();
  let target: string | undefined;
  try {
    const parsed = JSON.parse(content);
    // Most tools have one of these at the top level.
    target =
      parsed.path ||
      parsed.command ||
      parsed.pattern ||
      parsed.url ||
      parsed.query ||
      undefined;
    // fs_read uses `operations: [{ mode, path|image_paths }]` (or legacy `ops`).
    if (!target) {
      const ops = parsed.operations ?? parsed.ops;
      if (Array.isArray(ops) && ops.length > 0) {
        const first = ops[0];
        if (first && typeof first === 'object') {
          if (typeof first.path === 'string') {
            target = first.path;
          } else if (
            Array.isArray(first.image_paths) &&
            first.image_paths.length > 0
          ) {
            target = first.image_paths[0];
          }
        }
      }
    }
  } catch {
    target = undefined;
  }
  // Exclude fields already surfaced as the target to avoid duplication.
  const params = formatToolParams(content, [
    'path',
    'command',
    'pattern',
    'url',
    'query',
  ]);
  return (
    <Box flexDirection="column">
      <StatusInfo title={name} target={target} />
      <ToolMeta params={params} />
      <Box marginLeft={2}>
        <Text>{getColor('error')(error)}</Text>
      </Box>
    </Box>
  );
});
