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
import { ToolOutput } from '../chat/tools/ToolOutput.js';
import { formatToolParams } from '../../utils/tool-params.js';
import {
  parseToolArg,
  extractResultBodyText,
  splitBodyLines,
} from '../../utils/tool-result.js';
import { ToolUseStatus, type ToolResult } from '../../stores/app-store.js';
import {
  type ToolDiff,
  type ToolKind,
  type ToolCallLocation,
} from '../../types/agent-events.js';
import {
  resolveScrollbackToolRenderer,
  resolveToolId,
  type ToolCallOrigin,
} from '../../types/tool-capabilities.js';
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
  VerbosityOverrideContext,
  useVerboseDisplay,
  useShouldShowToolOutput,
} from '../../hooks/useVerbose.js';
import { extractToolReasoning } from '../../lite/render.js';
import { isMcpMessage } from '../../lite/verbose.js';
import {
  VerbosityToolContext,
  useToolOutputVisible,
  useVerbosityToolContext,
} from './VerbosityToolContext.js';
import { approvalDisplayConfig } from '../../lite/verbose.js';

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
  origin?: ToolCallOrigin;
  locations?: ToolCallLocation[];
  barColor?: string;
  /** Drop the solid accent-bar gutter (keeps the status dot). Used by the
   *  workflow monitor's session output, which must not show the left bar. */
  noBar?: boolean;
  isStatic?: boolean;
  agentLabel?: string;
  agentLabelColor?: string;
  purpose?: string;
  startTime?: number;
  finishTime?: number;
  /** MCP server hosting this tool (from `_meta.kiro.mcpServerName`); drives the
   *  `mcp` verbosity category since the TUI stores MCP tools under a bare name. */
  mcpServerName?: string;
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
    origin,
    locations,
    barColor,
    noBar,
    isStatic = false,
    agentLabel,
    agentLabelColor,
    purpose,
    startTime,
    finishTime,
    mcpServerName,
    denial,
  }) {
    const { getColor, wrapDisabled } = useTheme();
    const isLiteUi = useAppStore((s) => s.uiMode === 'lite');
    const keybindings = useKeybindings();
    const configuredDisplay = useVerboseDisplay();
    const configuredOutputVisible = useShouldShowToolOutput(
      name,
      isMcpMessage({ mcpServerName })
    );
    const isApproval = status === ToolUseStatus.Pending;
    const display = isApproval
      ? approvalDisplayConfig(configuredDisplay)
      : configuredDisplay;
    const outputVisible = isApproval || configuredOutputVisible;
    const renderer = resolveScrollbackToolRenderer(name, kind, origin);
    const toolOutputsExpanded = useAppStore((s) => s.toolOutputsExpanded);
    const frozenArgsExpanded = useRef(toolOutputsExpanded);
    if (!isStatic) frozenArgsExpanded.current = toolOutputsExpanded;
    const argsExpanded = isStatic
      ? frozenArgsExpanded.current
      : toolOutputsExpanded;
    const reasoning =
      display.showToolReasoning && renderer !== 'session'
        ? extractToolReasoning(content, purpose)
        : undefined;
    const elapsedMs =
      display.showElapsed && startTime != null && finishTime != null
        ? Math.max(0, finishTime - startTime)
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
        elapsedMs,
        argsMode: portActive ? display.toolArgsMode : undefined,
        argsMaxLines: portActive ? display.argsMaxLines : undefined,
        argsMaxChars: portActive ? display.argsMaxChars : undefined,
        argsExpanded,
        isStatic,
      }),
      [
        outputVisible,
        reasoning,
        elapsedMs,
        portActive,
        display.toolArgsMode,
        display.argsMaxLines,
        display.argsMaxChars,
        argsExpanded,
        isStatic,
      ]
    );

    const toolContent = (
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
          origin={origin}
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
    const inner = isApproval ? (
      <VerbosityOverrideContext.Provider value={{ display, filters: ['all'] }}>
        {toolContent}
      </VerbosityOverrideContext.Provider>
    ) : (
      toolContent
    );

    if (skipStatusBar) return inner;

    return (
      <StatusBar status={statusIcon} barColor={barColor} noBar={noBar}>
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
  origin?: ToolCallOrigin;
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
  if (
    shouldCollapseToolCard(
      props.name,
      props.kind,
      props.content,
      hideArgs,
      props.origin
    )
  ) {
    return <CollapsedToolEntry {...props} />;
  }
  return <FullToolContent {...props} />;
});

const CollapsedToolEntry = React.memo(function CollapsedToolEntry(
  props: ToolContentProps
) {
  const { name, kind, origin, content, isStatic } = props;
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const argsOff = useVerbosityToolContext().argsMode === 'off';
  const { expanded } = useExpandableOutput({
    totalItems: argsOff ? 0 : 2,
    previewCount: 1,
    isStatic,
  });

  if (expanded && !argsOff) return <FullToolContent {...props} />;

  const collapsed = collapsedToolPreview(name, kind, content, origin);
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
  origin,
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
  const renderer = resolveScrollbackToolRenderer(name, kind, origin);

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

  if (renderer === 'workflow') {
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
      if (renderer === 'session') {
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
      renderer !== 'write' &&
      renderer !== 'read' &&
      renderer !== 'image_read' &&
      renderer !== 'task';
    if (!toolRendersOwnError) {
      const toolId = resolveToolId(name, kind, origin);
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

  if (renderer === 'introspect') {
    return (
      <Introspect
        isFinished={effectiveFinished}
        isStatic={isStatic}
        content={content}
        result={result}
      />
    );
  }

  if (renderer === 'write') {
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

  if (renderer === 'read') {
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

  if (renderer === 'shell') {
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

  if (renderer === 'web_search') {
    return (
      <WebSearch
        isFinished={effectiveFinished}
        isStatic={isStatic}
        content={content}
        result={result}
      />
    );
  }

  if (renderer === 'web_fetch') {
    return (
      <WebFetch
        isFinished={effectiveFinished}
        isStatic={isStatic}
        content={content}
        result={result}
      />
    );
  }

  if (renderer === 'grep') {
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

  if (renderer === 'glob') {
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

  if (renderer === 'ls') {
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

  if (renderer === 'code') {
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

  if (renderer === 'session') {
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

  if (renderer === 'image_read') {
    return (
      <ImageRead
        noStatusBar
        isFinished={effectiveFinished}
        isStatic={isStatic}
        content={content}
      />
    );
  }

  if (renderer === 'goal') {
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

  if (renderer === 'task') {
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

  if (renderer === 'knowledge') {
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

  if (renderer !== 'generic') {
    const exhaustive: never = renderer;
    return exhaustive;
  }

  const toolId = resolveToolId(name, kind, origin);
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
  return <ToolOutput lines={lines} isStatic={isStatic} emptyPlaceholder />;
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
        <ToolOutput lines={error.split('\n')} isError />
      ) : (
        <Box marginLeft={2}>
          <Text>{getColor('error')(error)}</Text>
        </Box>
      )}
    </Box>
  );
});
