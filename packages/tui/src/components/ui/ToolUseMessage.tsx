import React, { useEffect, useMemo, useRef } from 'react';
import { Box, Text as InkText } from './../../renderer.js';
import { StatusBar, useStatusBar } from '../chat/status-bar/StatusBar.js';
import { StatusInfo } from './status/StatusInfo.js';
import type { StatusType } from '../../types/componentTypes.js';
import { Text } from './text/Text.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { ToolDenialDetails } from './ToolDenialDetails.js';
import type { ToolDenial } from '../../utils/tool-denial.js';
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
import { WorkflowTool } from '../chat/tools/WorkflowTool.js';
import { Tool } from '../chat/tools/Tool.js';
import { ToolMeta } from '../chat/tools/ToolMeta.js';
import {
  ToolOutput as ToolOutputBar,
  ToolOutputSection,
} from '../chat/tools/ToolOutput.js';
import { formatToolParams } from '../../utils/tool-params.js';
import {
  parseToolArg,
  extractResultBodyText,
  splitBodyLines,
} from '../../utils/tool-result.js';
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
  WORKFLOW_TOOL_NAMES,
  type ToolDiff,
  type ToolKind,
  type ToolCallLocation,
} from '../../types/agent-events.js';
import { getToolLabel } from '../../types/tool-status.js';
import { useKeybindings } from '../../hooks/useKeybindings.js';
import { useGlyphs } from '../../hooks/useGlyphs.js';
import { useExpandableOutput } from '../../hooks/useExpandableOutput.js';
import { useHideToolArgs } from './HideToolArgsContext.js';
import { MarkdownRenderer } from './MarkdownRenderer.js';
import {
  collapsedToolPreview,
  shouldCollapseToolCard,
} from '../../utils/collapsed-tool-view.js';
import { useAppStore } from '../../stores/app-store.js';
import {
  useVerboseDisplay,
  useShouldShowToolOutput,
} from '../../hooks/useVerbose.js';
import { extractToolReasoning } from '../../lite/render.js';
import {
  VerbosityToolContext,
  useToolOutputVisible,
  useVerbosityToolContext,
} from './VerbosityToolContext.js';

export interface ToolUseMessageProps {
  id: string;
  name: string;
  isQuestion?: boolean;
  content: string;
  diff?: ToolDiff;
  isFinished?: boolean;
  status?: ToolUseStatus;
  result?: ToolResult;
  kind?: ToolKind;
  locations?: ToolCallLocation[];
  barColor?: string;
  isStatic?: boolean;
  agentLabel?: string;
  agentLabelColor?: string;
  purpose?: string;
  startTime?: number;
  finishTime?: number;
  /** Denial detail for a blocked tool call (infra-safety or permission policy). */
  denial?: ToolDenial;
}

export const ToolUseMessage = React.memo<ToolUseMessageProps>(
  function ToolUseMessage({
    id,
    name,
    isQuestion,
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
    purpose,
    startTime,
    finishTime,
    denial,
  }) {
    const { getColor, wrapDisabled } = useTheme();
    const isLiteUi = useAppStore((s) => s.uiMode === 'lite');
    const keybindings = useKeybindings();
    const display = useVerboseDisplay();
    const outputVisible = useShouldShowToolOutput(name);
    const toolOutputsExpanded = useAppStore((s) => s.toolOutputsExpanded);
    const frozenArgsExpanded = useRef(toolOutputsExpanded);
    if (!isStatic) frozenArgsExpanded.current = toolOutputsExpanded;
    const argsExpanded = isStatic
      ? frozenArgsExpanded.current
      : toolOutputsExpanded;
    const reasoning =
      display.showToolReasoning && !SESSION_TOOL_NAMES.has(name)
        ? extractToolReasoning(content, purpose)
        : undefined;
    const elapsed =
      display.showElapsed && startTime != null && finishTime != null
        ? finishTime - startTime
        : undefined;
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
    const skipStatusBar = wrapDisabled || isLiteUi;

    const portActive = process.env.KIRO_LITE_ROLLOUT_ENABLED === '1';
    const toolContextValue = useMemo(
      () => ({
        outputVisible,
        reasoning,
        elapsedMs: elapsed,
        argsMode: portActive ? display.toolArgsMode : undefined,
        argsMaxLines: portActive ? display.argsMaxLines : undefined,
        argsMaxChars: portActive ? display.argsMaxChars : undefined,
        argsExpanded,
        isStatic,
      }),
      [
        outputVisible,
        reasoning,
        elapsed,
        portActive,
        display.toolArgsMode,
        display.argsMaxLines,
        display.argsMaxChars,
        argsExpanded,
        isStatic,
      ]
    );

    const inner = (
      <VerbosityToolContext.Provider value={toolContextValue}>
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
          isQuestion={isQuestion}
          kind={kind}
          content={content}
          diff={diff}
          isFinished={isFinished}
          status={status}
          result={result}
          isStatic={isStatic}
          locations={locations}
        />
        {denial && <ToolDenialDetails denial={denial} />}
        {showEscHint && (
          <Text>
            {getColor('muted')(
              `${keybindings.label('cancelStream')} to cancel`
            )}
          </Text>
        )}
      </VerbosityToolContext.Provider>
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
  isQuestion?: boolean;
  kind?: ToolKind;
  content: string;
  diff?: ToolDiff;
  isFinished: boolean;
  status?: ToolUseStatus;
  result?: ToolResult;
  isStatic: boolean;
  locations?: ToolCallLocation[];
}

const ToolUseContent = React.memo(function ToolUseContent(
  props: ToolContentProps
) {
  const hideArgs = useHideToolArgs();
  if (shouldCollapseToolCard(props.name, props.kind, props.content, hideArgs)) {
    return <CollapsedToolEntry {...props} />;
  }
  return <FullToolContent {...props} />;
});

const CollapsedToolEntry = React.memo(function CollapsedToolEntry(
  props: ToolContentProps
) {
  const { name, kind, content, isStatic } = props;
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const argsOff = useVerbosityToolContext().argsMode === 'off';
  const { expanded } = useExpandableOutput({
    totalItems: argsOff ? 0 : 2,
    previewCount: 1,
    isStatic,
  });

  if (expanded && !argsOff) return <FullToolContent {...props} />;

  const collapsed = collapsedToolPreview(name, kind, content);
  const { title, target } = collapsed;
  const preview = argsOff ? undefined : collapsed.preview;
  const muted = getColor('muted');
  const hint = getColor('secondary');
  const showMeta = !argsOff && (!!preview || !isStatic);

  return (
    <Box flexDirection="column">
      <StatusInfo title={title} target={target} />
      {showMeta && (
        <Box marginLeft={2}>
          <Text wrap="wrap">
            {muted(`${glyphs.cornerBottomLeftRound} `)}
            {preview && muted(preview)}
            {!isStatic &&
              !argsOff &&
              hint(`${preview ? ' ' : ''}(ctrl+o to expand)`)}
          </Text>
        </Box>
      )}
    </Box>
  );
});

const FullToolContent = React.memo(function FullToolContent({
  id,
  name,
  isQuestion,
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
  const { getColor } = useTheme();
  const glyphs = useGlyphs();

  const effectiveFinished = isFinished && status !== ToolUseStatus.Pending;

  useEffect(() => {
    requestRemeasure();
  }, [status, isFinished, requestRemeasure]);

  if (isQuestion) {
    const cancelled =
      status === ToolUseStatus.Rejected || result?.status === 'cancelled';
    return (
      <Box flexDirection="column">
        <MarkdownRenderer content={name} color={getColor('primary')} />
        {cancelled && <Text>{getColor('error')('Cancelled')}</Text>}
      </Box>
    );
  }

  if (WORKFLOW_TOOL_NAMES.has(name)) {
    return (
      <WorkflowTool
        name={name}
        isFinished={effectiveFinished}
        content={content}
        result={result}
        status={status}
      />
    );
  }

  if (status === ToolUseStatus.Rejected || result?.status === 'cancelled') {
    const label = result?.status === 'cancelled' ? 'Cancelled' : 'Rejected';
    try {
      const parsed = JSON.parse(content);
      let target: string;
      if (SESSION_TOOL_NAMES.has(name)) {
        const task = (['task', 'target', 'name'] as const)
          .map((k) => parsed[k])
          .find((v) => typeof v === 'string') as string | undefined;
        target = task
          ? `"${task.slice(0, 40)}${task.length > 40 ? '…' : ''}"`
          : name;
      } else {
        target = parsed.path || parsed.command || 'file';
      }
      return <StatusInfo title={label} target={target} />;
    } catch {
      return <StatusInfo title={label} target={name} />;
    }
  }

  if (result?.status === 'error' && effectiveFinished) {
    const toolRendersOwnError =
      !WRITE_TOOL_NAMES.has(name) &&
      kind !== 'edit' &&
      (INTROSPECT_TOOL_NAMES.has(name) ||
        (!READ_TOOL_NAMES.has(name) && kind !== 'read')) &&
      !IMAGE_READ_TOOL_NAMES.has(name) &&
      !TASK_TOOL_NAMES.has(name);
    if (!toolRendersOwnError) {
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

  if (WRITE_TOOL_NAMES.has(name) || kind === 'edit') {
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
        result={result}
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
        id={id}
        name={name}
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

  if (name === 'goal') {
    const labels: Record<string, string> = {
      complete: `${glyphs.checkmark} Goal complete`,
      status: 'Goal status',
    };
    let label = 'Goal';
    let detail: string | null = null;
    try {
      const parsed = JSON.parse(content);
      if (parsed.command && labels[parsed.command]) {
        label = labels[parsed.command]!;
      }
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
    const showBody = process.env.KIRO_LITE_ROLLOUT_ENABLED === '1';
    return (
      <>
        <StatusInfo title={label} />
        {showBody && (
          <ResultTextBody
            result={result}
            isFinished={effectiveFinished}
            isStatic={isStatic}
          />
        )}
      </>
    );
  }

  if (KNOWLEDGE_TOOL_NAMES.has(name)) {
    const title = getToolLabel('knowledge');
    const command = parseToolArg(content, 'command');
    const params = formatToolParams(content, ['command']);
    // Do not register expandable output off-cohort.
    const showBody = process.env.KIRO_LITE_ROLLOUT_ENABLED === '1';
    return (
      <>
        <StatusInfo title={title} target={command || undefined} />
        <ToolMeta params={params} />
        {showBody && (
          <ResultTextBody
            result={result}
            isFinished={effectiveFinished}
            isStatic={isStatic}
          />
        )}
      </>
    );
  }

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

const ResultTextBody = React.memo(function ResultTextBody({
  result,
  isFinished,
  isStatic,
}: {
  result?: ToolResult;
  isFinished: boolean;
  isStatic: boolean;
}) {
  const outputVisible = useToolOutputVisible();
  const text = extractResultBodyText(result);
  const lines = splitBodyLines(text);

  if (!isFinished || !outputVisible || result?.status !== 'success')
    return null;
  return (
    <ToolOutputSection
      lines={lines}
      isStatic={isStatic}
      previewCount={5}
      emptyPlaceholder
    />
  );
});

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
    target =
      parsed.path ||
      parsed.command ||
      parsed.pattern ||
      parsed.url ||
      parsed.query ||
      undefined;
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
      {process.env.KIRO_LITE_ROLLOUT_ENABLED === '1' ? (
        <ToolOutputBar lines={error.split('\n')} isError />
      ) : (
        <Box marginLeft={2}>
          <Text>{getColor('error')(error)}</Text>
        </Box>
      )}
    </Box>
  );
});
