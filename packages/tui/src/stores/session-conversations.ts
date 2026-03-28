/**
 * Session conversations store — standalone Zustand store.
 *
 * Holds per-session MessageType[] built by the shared message-stream-handler.
 * Components subscribe directly via useSessionConversation(sessionId).
 */

import { createStore, useStore } from 'zustand';
import { createMessageStreamHandler } from './message-stream-handler.js';
import type { MessageType } from './app-store.js';
import type { AgentStreamEvent } from '../types/agent-events.js';

interface SessionConversationsState {
  conversations: Map<string, MessageType[]>;
  createHandlerForSession: (
    sessionId: string
  ) => (event: AgentStreamEvent) => void;
  clearSession: (sessionId: string) => void;
}

const MAX_SESSION_MESSAGES = 50;

export const sessionConversationsStore = createStore<SessionConversationsState>(
  (set, get) => ({
    conversations: new Map(),

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
            m.set(
              sessionId,
              msgs.length > MAX_SESSION_MESSAGES
                ? msgs.slice(-MAX_SESSION_MESSAGES)
                : msgs
            );
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
