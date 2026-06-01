/**
 * Shared message stream handler — rendering logic only.
 *
 * Converts AgentStreamEvent into MessageType[] mutations.
 * Used by both the main chat (app-store) and the session conversations slice.
 * App-level side effects (approval, auth errors, compaction, etc.) stay in app-store.
 */

import {
  AgentEventType,
  deriveToolDiff,
  type AgentStreamEvent,
  type ToolDiff,
} from '../types/agent-events.js';
import { MessageRole, type MessageType } from './app-store.js';

export function createMessageStreamHandler(
  getMessages: () => MessageType[],
  setMessages: (updater: (msgs: MessageType[]) => MessageType[]) => void,
  getAgentName?: () => string | undefined
): (event: AgentStreamEvent) => void {
  let bufferedContent = '';
  let bufferedThinking = '';
  let pendingFlush: ReturnType<typeof setTimeout> | null = null;
  let lastContentId: string | null = null;

  const flushContent = () => {
    pendingFlush = null;
    if (!bufferedContent && !bufferedThinking) return;
    const content = bufferedContent;
    const thinking = bufferedThinking;
    setMessages((msgs) => {
      const last = msgs[msgs.length - 1];
      if (last?.role === MessageRole.Model) {
        return [
          ...msgs.slice(0, -1),
          {
            ...last,
            content: content || last.content,
            thinking: thinking || last.thinking,
            agentName: last.agentName ?? getAgentName?.(),
          },
        ];
      }
      return [
        ...msgs,
        {
          id: lastContentId ?? crypto.randomUUID(),
          role: MessageRole.Model,
          content,
          thinking: thinking || undefined,
          agentName: getAgentName?.(),
        },
      ];
    });
  };

  const flushNow = () => {
    if (pendingFlush) {
      clearTimeout(pendingFlush);
      pendingFlush = null;
    }
    flushContent();
  };

  return (event: AgentStreamEvent) => {
    switch (event.type) {
      case AgentEventType.Content:
        if (event.content.type === 'text') {
          bufferedContent += event.content.text;
          lastContentId = event.id;
          if (!pendingFlush) pendingFlush = setTimeout(flushContent, 16);
        }
        break;

      case AgentEventType.Thought:
        if (event.content.type === 'text') {
          bufferedThinking += event.content.text;
          lastContentId = event.id;
          if (!pendingFlush) pendingFlush = setTimeout(flushContent, 16);
        }
        break;

      case AgentEventType.ToolCall: {
        flushNow();
        bufferedContent = '';
        bufferedThinking = '';
        lastContentId = null;
        const content = JSON.stringify(event.args);
        const diff = deriveToolDiff(event);
        setMessages((msgs) => {
          const idx = msgs.findIndex(
            (m) => m.role === MessageRole.ToolUse && m.id === event.id
          );
          if (idx !== -1) {
            const existing = msgs[idx]!;
            if (
              existing.role === MessageRole.ToolUse &&
              (Object.keys(event.args).length > 0 || event.toolContent)
            ) {
              const next = [...msgs];
              next[idx] = {
                ...existing,
                content,
                kind: event.kind || existing.kind,
                locations: event.locations || existing.locations,
                diff: diff ?? existing.diff,
              };
              return next;
            }
            return msgs;
          }
          return [
            ...msgs,
            {
              id: event.id,
              role: MessageRole.ToolUse,
              name: event.name,
              kind: event.kind,
              content,
              diff,
              locations: event.locations,
              agentName: getAgentName?.(),
            },
          ];
        });
        break;
      }

      case AgentEventType.ToolCallUpdate:
        // Live output is handled by the liveOutputs Map in the main store.
        break;

      case AgentEventType.ToolCallFinished:
        setMessages((msgs) => {
          const idx = msgs.findIndex(
            (m) => m.role === MessageRole.ToolUse && m.id === event.id
          );
          if (idx === -1) return msgs;
          const msg = msgs[idx]!;
          if (msg.role !== MessageRole.ToolUse) return msgs;
          // Preserve user-initiated cancellation status — don't let a backend
          // ToolCallFinished (e.g. KAS sends status:'failed' for a cancelled
          // tool) overwrite a locally-set 'cancelled' result.
          if (msg.isFinished && msg.result?.status === 'cancelled') return msgs;
          // If the finished event carries diff content, attach it so <Write>
          // can render the post-write diff. Otherwise keep whatever was set
          // from the initial tool_call.
          const wireDiff = event.toolContent?.[0];
          const diff: ToolDiff | undefined = wireDiff
            ? {
                path: wireDiff.path,
                newText: wireDiff.newText,
                oldText: wireDiff.oldText,
              }
            : msg.diff;
          const next = [...msgs];
          next[idx] = {
            ...msg,
            diff,
            isFinished: true,
            result: event.result,
          };
          return next;
        });
        break;
    }
  };
}
