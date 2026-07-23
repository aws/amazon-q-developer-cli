import React, { useMemo } from 'react';
import { Box } from '../../../renderer.js';
import { Text } from '../text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { ToolUseMessage } from '../ToolUseMessage.js';
import { Message, MessageType } from '../../chat/message/Message.js';
import { ThinkingDisplay } from '../../chat/message/ThinkingDisplay.js';
import { StatusInfo } from '../status/StatusInfo.js';
import { SubagentDetail } from '../../chat/tools/SubagentDetail.js';
import { VerbosityOverrideContext } from '../../../hooks/useVerbose.js';
import { VerbosityToolContext } from '../VerbosityToolContext.js';
import {
  getVerbosityPreviewFixtures,
  PREVIEW_SUBAGENT_SUMMARIES,
  type VerbosityPreviewKey,
  type MessageLike,
  type SubagentStageSummary,
} from '../../../lite/render.js';
import {
  getTuiVerboseDisplay,
  getTuiVerboseFilters,
  shouldShowToolOutput,
  type VerboseDisplayConfig,
  type ThinkingDisplayMode,
} from '../../../lite/verbose.js';
import { SESSION_TOOL_NAMES } from '../../../types/agent-events.js';
import { useAppStore } from '../../../stores/app-store.js';

function toolProps(msg: MessageLike) {
  return {
    id: msg.id,
    name: msg.name ?? 'tool',
    content: msg.content,
    isFinished: msg.isFinished ?? true,
    isStatic: true,
    result: msg.result as never,
    purpose: msg.purpose,
    startTime: msg.startTime,
    finishTime: msg.finishTime,
  };
}

const PreviewRow = React.memo(function PreviewRow({
  msg,
  display,
  summaries,
  isKas,
  showFullOutput,
}: {
  msg: MessageLike;
  display: VerboseDisplayConfig;
  summaries: readonly SubagentStageSummary[];
  isKas: boolean;
  showFullOutput: boolean;
}) {
  const { getColor } = useTheme();
  const thinkingMode: ThinkingDisplayMode = display.thinkingDisplay;

  if (msg.role === 'user') {
    return (
      <Box>
        <Text>{getColor('secondary')('› ')}</Text>
        <Text>{msg.content}</Text>
      </Box>
    );
  }

  if (msg.role === 'model') {
    return (
      <Box flexDirection="column">
        {thinkingMode !== 'off' && msg.thinking && (
          <ThinkingDisplay text={msg.thinking} mode={thinkingMode} isStatic />
        )}
        {msg.content && (
          <Message content={msg.content} type={MessageType.AGENT} />
        )}
      </Box>
    );
  }

  if (msg.role === 'tool_use') {
    // Preview summaries are fixtures, not app-store messages.
    if (msg.name && SESSION_TOOL_NAMES.has(msg.name)) {
      return (
        <VerbosityToolContext.Provider
          value={{ outputVisible: true, argsMode: display.toolArgsMode }}
        >
          <Box flexDirection="column">
            <StatusInfo
              title="Orchestrated"
              target={`(${summaries.length} agent${summaries.length !== 1 ? 's' : ''})`}
              bold
              underline
            />
            <SubagentDetail
              content={msg.content}
              summaries={summaries}
              display={display}
              finished
              isKas={isKas}
              showFullOutput={showFullOutput}
            />
          </Box>
        </VerbosityToolContext.Provider>
      );
    }
    return <ToolUseMessage {...toolProps(msg)} />;
  }

  return null;
});

export interface TuiVerbosityPreviewProps {
  which: VerbosityPreviewKey;
  displayOverride?: VerboseDisplayConfig;
  filtersOverride?: readonly string[];
}

export const TuiVerbosityPreview: React.FC<TuiVerbosityPreviewProps> = ({
  which,
  displayOverride,
  filtersOverride,
}) => {
  const display = displayOverride ?? getTuiVerboseDisplay();
  const filters = filtersOverride ?? getTuiVerboseFilters();
  const isKas = useAppStore((s) => s.agentEngine === 'kas');
  const { messages, previewFilters } = useMemo(
    () => getVerbosityPreviewFixtures(which, filters),
    [which, filters]
  );

  const summaries = useMemo(() => {
    const base = PREVIEW_SUBAGENT_SUMMARIES;
    return isKas
      ? base.map((s) => ({ ...s, kind: 'response' as const }))
      : base;
  }, [isKas]);

  const override = useMemo(
    () => ({ display, filters: previewFilters }),
    [display, previewFilters]
  );
  const showFullOutput = shouldShowToolOutput('subagent', previewFilters);

  return (
    <VerbosityOverrideContext.Provider value={override}>
      <Box flexDirection="column">
        {messages.map((msg) => (
          <PreviewRow
            key={msg.id}
            msg={msg}
            display={display}
            summaries={summaries}
            isKas={isKas}
            showFullOutput={showFullOutput}
          />
        ))}
      </Box>
    </VerbosityOverrideContext.Provider>
  );
};
