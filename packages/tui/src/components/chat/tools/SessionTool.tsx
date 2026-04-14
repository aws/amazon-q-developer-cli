import React, { useMemo } from 'react';
import { Box } from '../../../renderer.js';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { StatusInfo } from '../../ui/status/StatusInfo.js';
import { parseToolArg } from '../../../utils/tool-result.js';
import type { ToolResult } from '../../../stores/app-store.js';

export interface SessionToolProps {
  name?: string;
  isFinished?: boolean;
  isStatic?: boolean;
  content?: string;
  result?: ToolResult;
}

/** Action labels: [in-progress, done] */
const ACTION_LABELS: Record<string, [string, string]> = {
  spawn_session: ['Spawning agent', 'Spawned agent'],
  send_message: ['Sending message', 'Sent message'],
  read_messages: ['Reading inbox', 'Read inbox'],
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
  name,
  isFinished = false,
  isStatic: _isStatic = false,
  content,
  result,
}: SessionToolProps) {
  const { getColor } = useTheme();

  const isCrewTool = name === 'subagent' || name === 'agent_crew';

  const action = useMemo(() => parseToolArg(content, 'action'), [content]);
  const target = useMemo(() => {
    if (isCrewTool) {
      const task = parseToolArg(content, 'task');
      return task
        ? `"${task.slice(0, 40)}${task.length > 40 ? '…' : ''}"`
        : undefined;
    }
    // For session_management: show the target session name or task
    const sessionName =
      parseToolArg(content, 'name') ?? parseToolArg(content, 'target');
    const task = parseToolArg(content, 'task');
    return (
      sessionName ??
      (task
        ? `"${task.slice(0, 40)}${task.length > 40 ? '…' : ''}"`
        : undefined)
    );
  }, [content, isCrewTool]);

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

  // For read_messages: show message count from result
  const messageCount = useMemo(() => {
    if (action !== 'read_messages' || !isFinished) return null;
    try {
      const parsed = JSON.parse(
        result?.status === 'success' ? String(result.output ?? '{}') : '{}'
      );
      const msgs = parsed.messages as any[] | undefined;
      return msgs?.length ?? null;
    } catch {
      return null;
    }
  }, [action, isFinished, result]);

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
      {isFinished && messageCount != null && (
        <Box marginLeft={2}>
          <Text>
            {getColor('secondary')(
              `${messageCount} message${messageCount !== 1 ? 's' : ''}`
            )}
          </Text>
        </Box>
      )}
    </Box>
  );
});
