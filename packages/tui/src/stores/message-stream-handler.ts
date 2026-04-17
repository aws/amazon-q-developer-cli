/**
 * Shared message stream handler — rendering logic only.
 *
 * Converts AgentStreamEvent into MessageType[] mutations.
 * Used by both the main chat (app-store) and the session conversations slice.
 * App-level side effects (approval, auth errors, compaction, etc.) stay in app-store.
 */

import {
  AgentEventType,
  type AgentStreamEvent,
} from '../types/agent-events.js';
import { MessageRole, type MessageType } from './app-store.js';

export function createMessageStreamHandler(
  getMessages: () => MessageType[],
  setMessages: (updater: (msgs: MessageType[]) => MessageType[]) => void,
  getAgentName?: () => string | undefined
): (event: AgentStreamEvent) => void {
  let bufferedContent = '';
  let pendingFlush: ReturnType<typeof setTimeout> | null = null;
  let lastContentId: string | null = null;

  const flushContent = () => {
    pendingFlush = null;
    if (!bufferedContent) return;
    const content = bufferedContent;
    setMessages((msgs) => {
      const last = msgs[msgs.length - 1];
      if (last?.role === MessageRole.Model) {
        return [
          ...msgs.slice(0, -1),
          { ...last, content, agentName: last.agentName ?? getAgentName?.() },
        ];
      }
      return [
        ...msgs,
        {
          id: lastContentId ?? crypto.randomUUID(),
          role: MessageRole.Model,
          content,
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

  const buildToolContent = (
    event: AgentStreamEvent & { type: typeof AgentEventType.ToolCall }
  ): string => {
    const diff = event.toolContent?.[0];
    if (diff) {
      const args = event.args as Record<string, unknown>;
      let command = 'create';
      if (args.oldStr !== undefined) command = 'strReplace';
      else if (args.insertLine !== undefined || (args as any).append)
        command = 'insert';
      return JSON.stringify({
        command,
        path: diff.path,
        content: diff.newText,
        oldStr: diff.oldText,
        newStr: diff.newText,
        insertLine: args.insertLine,
      });
    }
    if (event.kind === 'edit') {
      const args = event.args as Record<string, unknown>;
      let command = 'create';
      if (args.oldStr !== undefined) command = 'strReplace';
      else if (args.insertLine !== undefined || (args as any).append)
        command = 'insert';
      return JSON.stringify({
        command,
        path: args.path,
        content: args.text || args.content || '',
        oldStr: args.oldStr,
        newStr: args.newStr,
        insertLine: args.insertLine,
      });
    }
    return JSON.stringify(event.args);
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

      case AgentEventType.ToolCall: {
        flushNow();
        bufferedContent = '';
        lastContentId = null;
        const content = buildToolContent(event);
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
              locations: event.locations,
              agentName: getAgentName?.(),
            },
          ];
        });
        break;
      }

      case AgentEventType.ToolCallUpdate:
        if (event.content.type === 'text') {
          const text = event.content.text;
          setMessages((msgs) => {
            const idx = msgs.findIndex(
              (m) => m.role === MessageRole.ToolUse && m.id === event.id
            );
            if (idx === -1) return msgs;
            const msg = msgs[idx]!;
            if (msg.role !== MessageRole.ToolUse) return msgs;
            const next = [...msgs];
            next[idx] = { ...msg, liveOutput: (msg.liveOutput ?? '') + text };
            return next;
          });
        }
        break;

      case AgentEventType.ToolCallFinished:
        setMessages((msgs) => {
          const idx = msgs.findIndex(
            (m) => m.role === MessageRole.ToolUse && m.id === event.id
          );
          if (idx === -1) return msgs;
          const msg = msgs[idx]!;
          if (msg.role !== MessageRole.ToolUse) return msgs;
          const next = [...msgs];
          next[idx] = {
            ...msg,
            isFinished: true,
            result: event.result,
            liveOutput: undefined,
          };
          return next;
        });
        break;
    }
  };
}
