import React, { useMemo } from 'react';
import { useStore } from 'zustand';
import { Box } from '../../../renderer.js';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useGlyphs } from '../../../hooks/useGlyphs.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import { parseToolArg } from '../../../utils/tool-result.js';
import { useAppStore, type ToolResult } from '../../../stores/app-store.js';
import { sessionConversationsStore } from '../../../stores/session-conversations.js';
import { useVerboseDisplay } from '../../../hooks/useVerbose.js';
import { useToolOutputVisible } from '../../ui/VerbosityToolContext.js';
import { SubagentDetail, type SubagentDetailProps } from './SubagentDetail.js';
import { collectSubagentSummariesByParentCached } from '../../layout/lite/subagent-summaries.js';
import type { SubagentStageSummary } from '../../../lite/render.js';

const EMPTY: SubagentStageSummary[] = [];

export interface SessionToolProps {
  id?: string;
  name?: string;
  isFinished?: boolean;
  isStatic?: boolean;
  content?: string;
  result?: ToolResult;
}

/** Action labels: [in-progress, done] */
const ACTION_LABELS: Record<string, [string, string]> = {
  spawn_session: ['Spawning agent', 'Spawned agent'],
  list_sessions: ['Listing sessions', 'Listed sessions'],
  get_session_status: ['Checking session', 'Checked session'],
  interrupt: ['Interrupting session', 'Interrupted session'],
  inject_context: ['Injecting context', 'Injected context'],
  manage_group: ['Managing group', 'Managed group'],
  revive_session: ['Reviving session', 'Revived session'],
  register_pending_stages: ['Registering stages', 'Registered stages'],
};

/** Agent crew action labels */
const CREW_LABELS: [string, string] = ['Orchestrating', 'Orchestrated'];

export const SessionTool = React.memo(function SessionTool({
  id,
  name,
  isFinished = false,
  isStatic = false,
  content,
  result,
}: SessionToolProps) {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();

  const isCrewTool =
    name === 'subagent' ||
    name === 'agent_crew' ||
    name === 'orchestrate_subagent';

  const showSubagentDetail =
    isCrewTool && process.env.KIRO_LITE_ROLLOUT_ENABLED === '1';
  const detailFinished = isFinished && result?.status === 'success';
  const subagentDetail = showSubagentDetail && (
    <SessionSubagentDetail
      id={id}
      content={content}
      finished={detailFinished}
      isStatic={isStatic}
    />
  );

  const action = useMemo(() => parseToolArg(content, 'action'), [content]);
  const target = useMemo(() => {
    if (isCrewTool) {
      const task = parseToolArg(content, 'task');
      return task
        ? `"${task.slice(0, 40)}${task.length > 40 ? glyphs.ellipsis : ''}"`
        : undefined;
    }
    // For session_management: show the target session name or task
    const sessionName =
      parseToolArg(content, 'name') ?? parseToolArg(content, 'target');
    const task = parseToolArg(content, 'task');
    return (
      sessionName ??
      (task
        ? `"${task.slice(0, 40)}${task.length > 40 ? glyphs.ellipsis : ''}"`
        : undefined)
    );
  }, [content, isCrewTool, glyphs.ellipsis]);

  const [inProgressLabel, doneLabel] = isCrewTool
    ? CREW_LABELS
    : action
      ? (ACTION_LABELS[action] ?? [`Using ${action}`, `Used ${action}`])
      : ['Using session tool', 'Used session tool'];

  const title = isFinished ? doneLabel : inProgressLabel;

  // For crew tool: show agent count
  const agentCount = useMemo(() => {
    if (!isCrewTool) return null;
    try {
      const parsed = JSON.parse(content ?? '{}');
      const stages = parsed.stages as any[] | undefined;
      return stages?.length ?? null;
    } catch {
      return null;
    }
  }, [content, isCrewTool]);

  if (result?.status === 'error') {
    return (
      <Box flexDirection="column">
        <StatusInfo
          title={title}
          target={target}
          shimmer={false}
          bold={isCrewTool}
          underline={isCrewTool}
        />
        {subagentDetail}
        <Box marginLeft={2}>
          <Text>{getColor('error')(result.error)}</Text>
        </Box>
      </Box>
    );
  }

  return (
    <Box flexDirection="column">
      <StatusInfo
        title={title}
        target={
          agentCount != null
            ? `(${agentCount} agent${agentCount !== 1 ? 's' : ''})`
            : target
        }
        shimmer={!isFinished}
        bold={isCrewTool}
        underline={isCrewTool}
      />
      {subagentDetail}
    </Box>
  );
});

interface SessionSubagentDetailProps {
  id?: string;
  content?: string;
  finished: boolean;
  isStatic: boolean;
}

type DigestDetailProps = Omit<SubagentDetailProps, 'summaries'> & {
  id: string;
};

const SessionSubagentDetail = React.memo(function SessionSubagentDetail({
  id,
  content,
  finished,
  isStatic,
}: SessionSubagentDetailProps) {
  const display = useVerboseDisplay();
  const showFullOutput = useToolOutputVisible();
  const canShowDigests =
    !!id &&
    finished &&
    (!isStatic || display.persistOutput) &&
    (showFullOutput || display.subagent.responses);
  const isKas = useAppStore((s) => canShowDigests && s.agentEngine === 'kas');
  const collect = canShowDigests && (showFullOutput || !isKas);
  const detail = {
    content,
    display,
    finished,
    isStatic,
    showFullOutput,
    isKas,
  };
  if (collect) return <CollectedSubagentDetail id={id} {...detail} />;
  return <SubagentDetail {...detail} summaries={EMPTY} />;
});

const CollectedSubagentDetail = React.memo(function CollectedSubagentDetail({
  id,
  ...props
}: DigestDetailProps) {
  const messages = useAppStore((s) => s.messages);
  const sessions = useAppStore((s) => s.sessions);
  const agentName = useAppStore((s) => s.currentAgent?.name ?? null);
  const conversations = useStore(
    sessionConversationsStore,
    (s) => s.conversations
  );
  const summaries = useMemo(
    () =>
      collectSubagentSummariesByParentCached(
        messages,
        sessions,
        conversations,
        agentName
      ).get(id) ?? EMPTY,
    [id, messages, sessions, conversations, agentName]
  );
  return <SubagentDetail {...props} summaries={summaries} />;
});
