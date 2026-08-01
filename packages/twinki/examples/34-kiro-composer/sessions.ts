import { useCallback, useEffect, useRef, useState } from 'react';
import type { ShowcaseClient } from '../32-acp-showcase/acp-client.js';
import { initialSessionState, sessionReducer, type SessionAction } from '../32-acp-showcase/session-state.js';
import type { SessionState } from '../32-acp-showcase/types.js';

export interface ManagedSession {
  id: string;
  title: string;
  state: SessionState;
  draft: string;
  queued: string[];
}

export interface SessionSeed {
  id: string;
  title: string;
}

function initialSession(id: string, title: string): ManagedSession {
  return {
    id,
    title,
    state: { ...initialSessionState, blocks: [] },
    draft: '',
    queued: [],
  };
}

export function useManagedSessions(
  primaryClient: ShowcaseClient,
  createClient: () => ShowcaseClient,
  initialSessions: readonly SessionSeed[] = []
): {
  sessions: Record<string, ManagedSession>;
  ensureSession: (id: string, title: string) => void;
  createSession: (title?: string) => string;
  renameSession: (id: string, title: string) => void;
  closeSession: (id: string) => void;
  updateSession: (id: string, update: (session: ManagedSession) => ManagedSession) => void;
  dispatchSession: (id: string, action: SessionAction) => void;
  clientFor: (id: string) => ShowcaseClient | undefined;
} {
  const [sessions, setSessions] = useState<Record<string, ManagedSession>>(() =>
    Object.fromEntries(initialSessions.map(({ id, title }) => [id, initialSession(id, title)]))
  );
  const clients = useRef(new Map<string, ShowcaseClient>());
  const subscriptions = useRef(new Map<string, () => void>());
  const primaryAvailable = useRef(true);
  const nextSession = useRef(2);

  const updateSession = useCallback((id: string, update: (session: ManagedSession) => ManagedSession): void => {
    setSessions((current) => {
      const session = current[id];
      if (!session) return current;
      return { ...current, [id]: update(session) };
    });
  }, []);

  const dispatchSession = useCallback(
    (id: string, action: SessionAction): void => {
      updateSession(id, (session) => ({
        ...session,
        state: sessionReducer(session.state, action),
      }));
    },
    [updateSession]
  );

  const ensureSession = useCallback(
    (id: string, title: string): void => {
      if (clients.current.has(id)) {
        return;
      }
      const client = primaryAvailable.current ? primaryClient : createClient();
      primaryAvailable.current = false;
      clients.current.set(id, client);
      setSessions((current) => ({
        ...current,
        [id]: current[id] ?? initialSession(id, title),
      }));
      subscriptions.current.set(
        id,
        client.subscribe((action) => dispatchSession(id, action))
      );
      void client.start().catch((error: unknown) => {
        dispatchSession(id, {
          type: 'error',
          message: error instanceof Error ? error.message : String(error),
        });
      });
    },
    [createClient, dispatchSession, primaryClient, updateSession]
  );

  const createSession = useCallback(
    (title?: string): string => {
      let id: string;
      do id = `chat-${nextSession.current++}`;
      while (clients.current.has(id));
      ensureSession(id, title ?? `Session ${nextSession.current - 1}`);
      return id;
    },
    [ensureSession]
  );

  const renameSession = useCallback(
    (id: string, title: string): void => {
      const next = title.trim();
      if (next) updateSession(id, (session) => ({ ...session, title: next }));
    },
    [updateSession]
  );

  const closeSession = useCallback(
    (id: string): void => {
      subscriptions.current.get(id)?.();
      subscriptions.current.delete(id);
      const closing = clients.current.get(id);
      clients.current.delete(id);
      if (closing && closing !== primaryClient) void closing.close().catch(() => {});
      setSessions((current) => {
        const next = { ...current };
        delete next[id];
        return next;
      });
    },
    [primaryClient]
  );

  useEffect(
    () => () => {
      for (const unsubscribe of subscriptions.current.values()) unsubscribe();
      for (const client of clients.current.values()) {
        if (client !== primaryClient) void client.close().catch(() => {});
      }
    },
    [primaryClient]
  );

  const clientFor = useCallback((id: string) => clients.current.get(id), []);

  return {
    sessions,
    ensureSession,
    createSession,
    renameSession,
    closeSession,
    updateSession,
    dispatchSession,
    clientFor,
  };
}
