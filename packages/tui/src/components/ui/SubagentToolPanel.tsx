import React, { useMemo, useRef } from 'react';
import { Box } from './../../renderer.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { getAgentColor } from '../../utils/agentColors.js';
import { Text } from '../ui/text/Text.js';
import { Icon, IconType } from '../ui/icon/Icon.js';
import { PieSpinner } from '../ui/spinner/PieSpinner.js';
import { getStatusColor } from '../../utils/colorUtils.js';
import { useAppStore, MessageRole } from '../../stores/app-store.js';
import { useKeypress } from '../../hooks/useKeypress.js';
import { resolveToolId } from '../../types/agent-events.js';
import { getToolLabel } from '../../types/tool-status.js';
import type { AgentSession } from '../../types/multi-session.js';

interface SubagentToolPanelProps {
  isStatic?: boolean;
}

const MAX_TOOL_COL = 50;

function getToolParam(content: string): string | null {
  try {
    const parsed = JSON.parse(content);
    return (
      parsed.path ||
      parsed.command ||
      parsed.pattern ||
      parsed.query ||
      parsed.symbol_name ||
      parsed.url ||
      null
    );
  } catch {
    return null;
  }
}

function formatToolDesc(name: string, content: string): string {
  const toolId = resolveToolId(name);
  const label = toolId ? getToolLabel(toolId) : name;
  const param = getToolParam(content);
  const desc = param ? `${label} (${param})` : label;
  if (desc.length > MAX_TOOL_COL)
    return desc.slice(0, MAX_TOOL_COL - 3) + '...';
  return desc;
}

interface AgentRow {
  name: string;
  agentName: string;
  status: string;
  activeToolDesc: string | null;
  hasPendingApproval: boolean;
}

export const SubagentToolPanel = React.memo<SubagentToolPanelProps>(
  function SubagentToolPanel({ isStatic = false }) {
    const { getColor } = useTheme();
    const sessions = useAppStore((state) => state.sessions);
    const sessionId = useAppStore((state) => state.sessionId);
    const messages = useAppStore((state) => state.messages);
    const approvalQueue = useAppStore((state) => state.approvalQueue);
    const focusedCrewIndex = useAppStore((state) => state.focusedCrewIndex);
    const setFocusedCrewIndex = useAppStore(
      (state) => state.setFocusedCrewIndex
    );
    const orderRef = useRef<string[]>([]);

    const sessionsWithApproval = useMemo(
      () => new Set(approvalQueue.map((a) => a.sessionId)),
      [approvalQueue]
    );

    const rows = useMemo(() => {
      const subagentSessions: AgentSession[] = [];
      for (const s of sessions.values()) {
        if (s.id === sessionId) continue;
        if (s.id.startsWith('pending:')) continue;
        if (s.type !== 'ephemeral') continue;
        subagentSessions.push(s);
      }

      const activeToolByAgent = new Map<
        string,
        { name: string; content: string }
      >();
      for (const msg of messages) {
        if (msg.role !== MessageRole.ToolUse) continue;
        if (!msg.agentName) continue;
        if (!msg.isFinished) {
          activeToolByAgent.set(msg.agentName, {
            name: msg.name,
            content: msg.content,
          });
        }
      }

      // Maintain stable insertion order across renders, reset when sessions change entirely
      const currentNames = new Set(subagentSessions.map((s) => s.name));
      if (
        orderRef.current.length > 0 &&
        !orderRef.current.some((n) => currentNames.has(n))
      ) {
        orderRef.current = [];
      }
      for (const s of subagentSessions) {
        if (!orderRef.current.includes(s.name)) orderRef.current.push(s.name);
      }
      orderRef.current = orderRef.current.filter((n) => currentNames.has(n));

      const sessionByName = new Map(subagentSessions.map((s) => [s.name, s]));
      const result: AgentRow[] = [];
      for (const name of orderRef.current) {
        const session = sessionByName.get(name);
        if (!session) continue;
        const tool = activeToolByAgent.get(name);
        result.push({
          name,
          agentName: session.agentName ?? name,
          status: session.status,
          activeToolDesc: tool ? formatToolDesc(tool.name, tool.content) : null,
          hasPendingApproval: sessionsWithApproval.has(session.id),
        });
      }
      return result;
    }, [sessions, sessionId, messages, sessionsWithApproval]);

    // Clamp focused index to valid range
    const clampedIndex = Math.min(focusedCrewIndex, rows.length - 1);

    useKeypress((input, key) => {
      if (isStatic || rows.length === 0 || !key.ctrl) return;
      if (input === 'd') {
        setFocusedCrewIndex(Math.min(clampedIndex + 1, rows.length - 1));
      } else if (input === 'u') {
        setFocusedCrewIndex(Math.max(clampedIndex - 1, 0));
      }
    });

    if (rows.length === 0) return <Box />;

    const maxNameLen = Math.max(...rows.map((r) => r.name.length));
    const maxAgentLen = Math.max(...rows.map((r) => r.agentName.length));

    const allDone = rows.every(
      (r) => r.status === 'terminated' || r.status === 'failed'
    );
    const barColor = isStatic || allDone
      ? getColor('success').hex
      : getColor('brand').hex;

    return (
      <Box flexDirection="column">
        {!isStatic && rows.length > 1 && (
          <Box flexDirection="row">
            <Text backgroundColor={barColor}> </Text>
            <Text> {getColor('secondary')('ctrl+d/u navigate · ctrl+g monitor')}</Text>
          </Box>
        )}
        {rows.map((row, i) => {
          const isDone = row.status === 'terminated' || row.status === 'failed';
          const isError = row.status === 'failed';
          const agentColor = getAgentColor(row.agentName, getColor);
          const isFocused = !isStatic && i === clampedIndex;

          let statusText: string;
          let statusColor: string;
          if (isDone) {
            statusText = isError ? 'Failed' : 'Completed';
            statusColor = isError ? 'error' : 'success';
          } else if (row.hasPendingApproval) {
            statusText = '⚠ tool approval needed';
            statusColor = 'warning';
          } else if (row.activeToolDesc) {
            statusText = row.activeToolDesc;
            statusColor = 'secondary';
          } else {
            statusText = 'Thinking...';
            statusColor = 'secondary';
          }

          return (
            <Box key={row.name} flexDirection="row">
              <Text backgroundColor={barColor}> </Text>
              <Box marginLeft={1} flexDirection="row" gap={1}>
              <AgentBullet
                isDone={isDone}
                isError={isError}
                isStatic={isStatic}
                getColor={getColor}
                agentName={row.agentName}
              />
              <Text>
                {isFocused
                  ? getColor('primary').bold.underline(
                      row.name.padEnd(maxNameLen)
                    )
                  : getColor('primary')(row.name.padEnd(maxNameLen))}
              </Text>
              <Text>{agentColor(row.agentName.padEnd(maxAgentLen))}</Text>
              <Text>{getColor(statusColor)(statusText)}</Text>
              </Box>
            </Box>
          );
        })}
      </Box>
    );
  }
);

const AgentBullet = React.memo(function AgentBullet({
  isDone,
  isError,
  isStatic,
  getColor,
  agentName,
}: {
  isDone: boolean;
  isError: boolean;
  isStatic: boolean;
  getColor: (colorPath: string) => any;
  agentName: string;
}) {
  if (isDone) {
    const color = isError
      ? getStatusColor('error', getColor)
      : getStatusColor('success', getColor);
    return <Icon type={IconType.DOT} color={color} />;
  }
  const agentColor = getAgentColor(agentName, getColor);
  if (isStatic) {
    return <Icon type={IconType.DOT} color={agentColor} />;
  }
  return <PieSpinner color={agentColor} />;
});
