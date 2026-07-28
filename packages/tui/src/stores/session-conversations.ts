/**
 * Session conversations store — standalone Zustand store.
 *
 * Holds per-session MessageType[] built by the shared message-stream-handler.
 * Components subscribe directly via useSessionConversation(sessionId).
 */

import { createStore, useStore } from 'zustand';
import { createMessageStreamHandler } from './message-stream-handler.js';
import { MessageRole, type MessageType } from './app-store.js';
import type { AgentStreamEvent } from '../types/agent-events.js';

export interface SessionConversationWriter {
  appendLocalUserMessage(sessionId: string, content: string): void;
}

interface SessionConversationsState extends SessionConversationWriter {
  conversations: Map<string, MessageType[]>;
  createHandlerForSession: (
    sessionId: string
  ) => (event: AgentStreamEvent) => void;
  clearSession: (sessionId: string) => void;
}

const MAX_SESSION_MESSAGES = 50;

function boundedMessages(messages: MessageType[]): MessageType[] {
  return messages.length > MAX_SESSION_MESSAGES
    ? messages.slice(-MAX_SESSION_MESSAGES)
    : messages;
}

export const sessionConversationsStore = createStore<SessionConversationsState>(
  (set, get) => ({
    conversations: new Map(),

    appendLocalUserMessage: (sessionId, content) =>
      set((state) => {
        const conversations = new Map(state.conversations);
        const messages = conversations.get(sessionId) ?? [];
        conversations.set(
          sessionId,
          boundedMessages([
            ...messages,
            {
              id: crypto.randomUUID(),
              role: MessageRole.User,
              content,
            },
          ])
        );
        return { conversations };
      }),

    clearSession: (sessionId) =>
      set((s) => {
        const m = new Map(s.conversations);
        m.delete(sessionId);
        return { conversations: m };
      }),

    createHandlerForSession: (sessionId) =>
      createMessageStreamHandler(
        () => get().conversations.get(sessionId) ?? [],
        (updater) =>
          set((s) => {
            const m = new Map(s.conversations);
            const msgs = updater(m.get(sessionId) ?? []);
            m.set(sessionId, boundedMessages(msgs));
            return { conversations: m };
          })
      ),
  })
);

const EMPTY: MessageType[] = [];

export function useSessionConversation(sessionId: string): MessageType[] {
  return useStore(
    sessionConversationsStore,
    (s) => s.conversations.get(sessionId) ?? EMPTY
  );
}
