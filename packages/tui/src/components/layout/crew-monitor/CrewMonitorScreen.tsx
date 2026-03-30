import React, { useState, useEffect, useMemo, useRef } from 'react';
import { useFullscreen, Box, Text } from '../../../renderer.js';
import { useTerminalSize } from '../../../hooks/useTerminalSize.js';
import { useAppStore, MessageRole } from '../../../stores/app-store.js';
import { sessionConversationsStore } from '../../../stores/session-conversations.js';
import { useStore } from 'zustand';
import { resolveToolId } from '../../../types/agent-events.js';
import { getToolLabel } from '../../../types/tool-status.js';
import type { Stage } from './types.js';
import { mapSessionStatusToStageState } from './types.js';
import { CrewMonitorLayout } from './CrewMonitorLayout.js';

/**
 * Derive a stable Set<string> of session IDs that have received at least one
 * event. We only care about the 0→>0 transition (to flip Pending→Completed),
 * so we keep a ref to the previous set and return the same reference when
 * nothing changed. This prevents the entire tree from re-rendering on every
 * streaming token.
 */
function useSessionsWithEvents(): Set<string> {
  const raw = useAppStore((state) => state.sessionEventBuffer);
  const prevRef = useRef<Set<string>>(new Set());

  return useMemo(() => {
    const next = new Set<string>();
    for (const key of Object.keys(raw)) {
      if (raw[key]!.length > 0) next.add(key);
    }
    // Return previous reference if the set hasn't changed
    const prev = prevRef.current;
    if (next.size === prev.size && [...next].every((v) => prev.has(v))) {
      return prev;
    }
    prevRef.current = next;
    return next;
  }, [raw]);
}

export const CrewMonitorScreen: React.FC = () => {
  useFullscreen();
  const [elapsed, setElapsed] = useState(0);
  const focusedCrewIndex = useAppStore((state) => state.focusedCrewIndex);
  const [selectedIndex, setSelectedIndex] = useState(focusedCrewIndex);
  const [startTime] = useState(Date.now());

  const sessionsMap = useAppStore((state) => state.sessions);
  const sessionsWithEvents = useSessionsWithEvents();
  const { width, height } = useTerminalSize();

  const sessions = useMemo(
    () => Array.from(sessionsMap.values()),
    [sessionsMap]
  );

  const stages: Stage[] = useMemo(() => {
    const unsorted = sessions.map((session) => ({
      name: session.name,
      agentName: (session as any).agentName ?? session.name,
      state: mapSessionStatusToStageState(session.status),
      description: (session as any).summary || session.role || 'Agent session',
      events: 0,
      role: session.role || 'agent',
      sessionId: session.id,
      group: (session as any).group,
      isPending: session.status === 'pending',
      dependsOn: (session as any).dependsOn ?? [],
    }));
    const placed = new Set<string>();
    const sorted: Stage[] = [];
    const remaining = [...unsorted];
    while (remaining.length > 0) {
      const next = remaining.findIndex((s) =>
        s.dependsOn.every((d: string) => placed.has(d))
      );
      if (next === -1) {
        sorted.push(...remaining);
        break;
      }
      const [stage] = remaining.splice(next, 1);
      placed.add(stage!.name);
      sorted.push(stage!);
    }
    return sorted;
  }, [sessions]);

  const conversations = useStore(
    sessionConversationsStore,
    (s) => s.conversations
  );
  const approvalQueue = useAppStore((state) => state.approvalQueue);
  const sessionsWithApproval = useMemo(
    () => new Set(approvalQueue.map((a) => a.sessionId)),
    [approvalQueue]
  );

  const stagesWithCounts = useMemo(
    () =>
      stages.map((s) => {
        const hasEvents = sessionsWithEvents.has(s.sessionId);
        const state =
          s.state === 'Pending' && hasEvents && !s.isPending
            ? 'Completed'
            : s.state;

        let activeStatus: string | undefined;
        if (sessionsWithApproval.has(s.sessionId)) {
          // Show the tool name requesting approval
          const msgs = conversations.get(s.sessionId);
          if (msgs) {
            for (let i = msgs.length - 1; i >= 0; i--) {
              const m = msgs[i]!;
              if (m.role === MessageRole.ToolUse && !m.isFinished) {
                const toolId = resolveToolId(m.name);
                activeStatus = toolId ? getToolLabel(toolId) : m.name;
                break;
              }
            }
          }
          if (!activeStatus) activeStatus = 'approval needed';
        } else if (state === 'Completed') {
          activeStatus = 'Completed';
        } else if (state === 'Failed') {
          activeStatus = 'Failed';
        } else if (state === 'Pending') {
          activeStatus = 'Waiting';
        } else if (state === 'Executing') {
          const msgs = conversations.get(s.sessionId);
          if (msgs) {
            for (let i = msgs.length - 1; i >= 0; i--) {
              const m = msgs[i]!;
              if (m.role === MessageRole.ToolUse && !m.isFinished) {
                const toolId = resolveToolId(m.name);
                const label = toolId ? getToolLabel(toolId) : m.name;
                let param: string | undefined;
                try {
                  const p = JSON.parse(m.content);
                  param = p.path || p.command || p.pattern || p.query;
                } catch {
                  /* ignore */
                }
                activeStatus = param ? `${label} (${param})` : label;
                break;
              }
            }
          }
          if (!activeStatus) activeStatus = 'Thinking...';
        }

        return { ...s, events: hasEvents ? 1 : 0, state, activeStatus };
      }),
    [stages, sessionsWithEvents, conversations, sessionsWithApproval]
  );

  useEffect(() => {
    const t = setInterval(
      () => setElapsed(Math.floor((Date.now() - startTime) / 1000)),
      1000
    );
    return () => clearInterval(t);
  }, [startTime]);

  const formatElapsed = (s: number) => `${Math.floor(s / 60)}m ${s % 60}s`;

  if (sessions.length === 0) {
    return (
      <Box flexDirection="column" width={width} height={height}>
        <Box paddingX={2}>
          <Text bold color="white">
            AGENT MONITOR
          </Text>
        </Box>
        <Box flexGrow={1} justifyContent="center" alignItems="center">
          <Box flexDirection="column" alignItems="center">
            <Text color="gray">No active subagents</Text>
            <Text color="gray">Press q or ^g to return to chat</Text>
          </Box>
        </Box>
      </Box>
    );
  }

  return (
    <CrewMonitorLayout
      stages={stagesWithCounts}
      selectedIndex={selectedIndex}
      onSelect={setSelectedIndex}
      elapsed={formatElapsed(elapsed)}
      width={width}
      height={height}
    />
  );
};
