import React, { useMemo, useRef } from 'react';
import { useStore } from 'zustand';
import { Box } from './../../renderer.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useGlyphs, useAllowIcons } from '../../hooks/useGlyphs.js';
import { getAgentColor } from '../../utils/agentColors.js';
import { Text } from '../ui/text/Text.js';
import { Icon, IconType } from '../ui/icon/Icon.js';
import { PieSpinner } from '../ui/spinner/PieSpinner.js';
import { getStatusColor } from '../../utils/colorUtils.js';
import {
  useAppStore,
  MessageRole,
  type MessageType,
} from '../../stores/app-store.js';
import { useKeypress } from '../../hooks/useKeypress.js';
import {
  resolveToolId,
  type ApprovalRequestInfo,
  type ToolCallOrigin,
  type ToolKind,
} from '../../types/agent-events.js';
import { sessionConversationsStore } from '../../stores/session-conversations.js';
import {
  isSubagentWrapperTool,
  resolveToolDisplayName,
} from '../../utils/collapsed-tool-view.js';
import { truncateToWidth, visibleWidth } from '../../utils/text-width.js';
import { ATTENTION_TEXT } from '../layout/crew-monitor/types.js';
import { selectSubagentToolSessions } from './subagent-session-filter.js';

interface SubagentToolPanelProps {
  isStatic?: boolean;
  /** KAS pipeline group owned by the parent orchestration tool. */
  pipelineGroupId?: string;
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

interface ToolIdentity {
  name: string;
  kind?: ToolKind;
  origin?: ToolCallOrigin;
}

function approvalToolIdentity(
  approval: ApprovalRequestInfo
): ToolIdentity | undefined {
  const { toolCall, toolId } = approval;
  const name = toolId ?? toolCall.name;
  if (!name) return undefined;

  const isDescriptiveTitleFallback =
    toolId === undefined &&
    toolCall.title !== undefined &&
    name === toolCall.title &&
    toolCall.origin !== 'mcp' &&
    resolveToolId(name, toolCall.kind, toolCall.origin) === undefined;
  if (isDescriptiveTitleFallback) return undefined;

  return { name, kind: toolCall.kind, origin: toolCall.origin };
}

function truncateToolStatus(text: string, maxWidth = MAX_TOOL_COL): string {
  return truncateToWidth(text, maxWidth, '...');
}

function formatToolDesc(
  name: string,
  content: string,
  kind?: ToolKind,
  origin?: ToolCallOrigin
): string {
  const label = resolveToolDisplayName(name, kind, origin);
  const param = getToolParam(content);
  return truncateToolStatus(param ? `${label} (${param})` : label);
}

function formatApprovalStatus(
  toolLabel: string | null,
  approvalText: string
): string {
  if (!toolLabel) return truncateToolStatus(approvalText);
  const labelWidth = MAX_TOOL_COL - visibleWidth(approvalText) - 1;
  return `${truncateToolStatus(toolLabel, labelWidth)} ${approvalText}`;
}

interface AgentRow {
  name: string;
  agentName: string;
  status: string;
  pendingApprovalToolLabel: string | null;
  activeToolDesc: string | null;
  hasPendingApproval: boolean;
}

export const SubagentToolPanel = React.memo<SubagentToolPanelProps>(
  function SubagentToolPanel({ isStatic = false, pipelineGroupId }) {
    const { getColor } = useTheme();
    const glyphs = useGlyphs();
    const { allowIcons } = useAllowIcons();
    const sessions = useAppStore((state) => state.sessions);
    const sessionId = useAppStore((state) => state.sessionId);
    const messages = useAppStore((state) => state.messages);
    const approvalQueue = useAppStore((state) => state.approvalQueue);
    const focusedCrewIndex = useAppStore((state) => state.focusedCrewIndex);
    const setFocusedCrewIndex = useAppStore(
      (state) => state.setFocusedCrewIndex
    );
    const orderRef = useRef<string[]>([]);
    // Capped buffers cost less than synchronizing narrower selectors.
    const conversations = useStore(
      sessionConversationsStore,
      (s) => s.conversations
    );

    const pendingApprovalBySessionId = useMemo(() => {
      const approvals = new Map<string, (typeof approvalQueue)[number]>();
      for (const approval of approvalQueue) {
        if (approval.sessionId && !approvals.has(approval.sessionId)) {
          approvals.set(approval.sessionId, approval);
        }
      }
      return approvals;
    }, [approvalQueue]);

    const rows = useMemo(() => {
      const subagentSessions = selectSubagentToolSessions(sessions.values(), {
        mainSessionId: sessionId,
        pipelineGroupId,
      });

      interface ActiveTool extends ToolIdentity {
        content: string;
      }

      function lastUnfinishedTool(
        msgs: readonly MessageType[]
      ): ActiveTool | undefined {
        for (let i = msgs.length - 1; i >= 0; i--) {
          const msg = msgs[i]!;
          if (msg.role !== MessageRole.ToolUse) continue;
          if (msg.isFinished) continue;
          // Wrapper cards represent the sub-agent itself, not its current work.
          if (isSubagentWrapperTool(msg.name, msg.kind, msg.origin)) continue;
          return {
            name: msg.name,
            content: msg.content,
            kind: msg.kind,
            origin: msg.origin,
          };
        }
        return undefined;
      }

      const approvalToolBySessionId = new Map<string, ToolIdentity>();
      const unresolvedApprovalToolIds = new Set<string>();
      for (const session of subagentSessions) {
        const approval = pendingApprovalBySessionId.get(session.id);
        if (!approval) continue;
        const identity = approvalToolIdentity(approval);
        if (identity) {
          approvalToolBySessionId.set(session.id, identity);
        } else if (approval.toolCall.toolCallId) {
          unresolvedApprovalToolIds.add(approval.toolCall.toolCallId);
        }
      }

      // Exact-id scans only bridge approval payloads missing canonical identity.
      const toolById = new Map<string, ToolIdentity>();
      const activeToolBySessionId = new Map<string, ActiveTool>();
      for (const [id, msgs] of conversations) {
        if (unresolvedApprovalToolIds.size > 0) {
          for (const msg of msgs) {
            if (
              msg.role !== MessageRole.ToolUse ||
              !unresolvedApprovalToolIds.has(msg.id)
            ) {
              continue;
            }
            toolById.set(msg.id, {
              name: msg.name,
              kind: msg.kind,
              origin: msg.origin,
            });
          }
        }
        const tool = lastUnfinishedTool(msgs);
        if (tool) activeToolBySessionId.set(id, tool);
      }

      // Main-store lookup covers pre-buffer events and calls evicted by the 50-message cap.
      const activeToolByAgent = new Map<string, ActiveTool>();
      for (const msg of messages) {
        if (msg.role !== MessageRole.ToolUse) continue;
        if (unresolvedApprovalToolIds.has(msg.id)) {
          toolById.set(msg.id, {
            name: msg.name,
            kind: msg.kind,
            origin: msg.origin,
          });
        }
        if (!msg.agentName) continue;
        if (msg.isFinished) continue;
        if (isSubagentWrapperTool(msg.name, msg.kind, msg.origin)) continue;
        activeToolByAgent.set(msg.agentName, {
          name: msg.name,
          content: msg.content,
          kind: msg.kind,
          origin: msg.origin,
        });
      }

      // Preserve row order while any known session remains.
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
        const tool =
          activeToolBySessionId.get(session.id) ?? activeToolByAgent.get(name);
        const approval = pendingApprovalBySessionId.get(session.id);
        const approvalTool = approval
          ? (approvalToolBySessionId.get(session.id) ??
            toolById.get(approval.toolCall.toolCallId))
          : undefined;
        result.push({
          name,
          agentName: session.agentName ?? name,
          status: session.status,
          pendingApprovalToolLabel: approvalTool
            ? resolveToolDisplayName(
                approvalTool.name,
                approvalTool.kind,
                approvalTool.origin
              )
            : null,
          activeToolDesc: tool
            ? formatToolDesc(tool.name, tool.content, tool.kind, tool.origin)
            : null,
          hasPendingApproval: approval !== undefined,
        });
      }
      return result;
    }, [
      sessions,
      sessionId,
      pipelineGroupId,
      messages,
      conversations,
      pendingApprovalBySessionId,
    ]);

    // Clamp focused index to valid range
    const clampedIndex = Math.min(focusedCrewIndex, rows.length - 1);

    useKeypress((input, key) => {
      if (isStatic || rows.length === 0 || !key.ctrl) return;
      if (input === 'd') {
        setFocusedCrewIndex((clampedIndex + 1) % rows.length);
      } else if (input === 'u') {
        setFocusedCrewIndex((clampedIndex - 1 + rows.length) % rows.length);
      }
    });

    if (rows.length === 0) return null;

    const maxNameLen = Math.max(...rows.map((r) => r.name.length));
    // Session name and agent name coincide for dispatched sub-agents; printing both
    // renders the name twice per row.
    const showAgentColumn = rows.some((r) => r.agentName !== r.name);
    const maxAgentLen = showAgentColumn
      ? Math.max(...rows.map((r) => r.agentName.length))
      : 0;

    const allDone = rows.every(
      (r) => r.status === 'terminated' || r.status === 'failed'
    );
    const barColor =
      isStatic || allDone ? getColor('success').hex : getColor('brand').hex;

    return (
      <Box flexDirection="column">
        {!isStatic && rows.length > 0 && (
          <Box flexDirection="row">
            <Text backgroundColor={barColor}> </Text>
            <Text>
              {' '}
              {getColor('secondary')(
                rows.length > 1
                  ? `ctrl+d/u navigate ${glyphs.smallDot} ctrl+g open agent monitor`
                  : 'ctrl+g open agent monitor'
              )}
            </Text>
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
            const approvalText = allowIcons
              ? `${glyphs.warning} ${ATTENTION_TEXT}`
              : ATTENTION_TEXT;
            statusText = formatApprovalStatus(
              row.pendingApprovalToolLabel,
              approvalText
            );
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
                {showAgentColumn && (
                  <Text>{agentColor(row.agentName.padEnd(maxAgentLen))}</Text>
                )}
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
