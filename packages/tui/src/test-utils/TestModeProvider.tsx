import React, { useEffect, useContext } from 'react';
import * as net from 'net';
import * as fs from 'fs';
import { AppStoreContext } from '../stores/app-store';
import type {
  SerializedAppState,
  TestCommand,
  TestResponse,
} from './shared/ipc-types';
import { TuiIpcConnection } from './shared/tui-ipc-connection';
import { getMockSessionClient } from './MockSessionClient';

interface TestModeProviderProps {
  children: React.ReactNode;
}

/**
 * TestModeProvider enables IPC communication between the TUI process and test cases.
 *
 * When KIRO_TEST_MODE is enabled, this provider establishes a Unix socket connection
 * to the test runner, allowing tests to inspect and manipulate the application state
 * in real-time while the TUI runs in an authentic terminal environment.
 *
 * The provider handles incoming test commands via IPC and provides access to:
 * - Current Zustand store state (for assertions)
 * - Error injection capabilities (for error scenario testing)
 * - Future extensibility for additional test operations
 *
 * This component is automatically included in the provider hierarchy and only
 * activates when the appropriate test environment variables are set.
 *
 * @example
 * ```typescript
 * // In test: Query the current application state
 * const state = await testCase.getStore();
 * expect(state.input.lines[0]).toBe('hello');
 *
 * // In test: Inject mock errors
 * await testCase.mockError('Connection failed');
 * ```
 */
export const TestModeProvider: React.FC<TestModeProviderProps> = ({
  children,
}) => {
  const appStore = useContext(AppStoreContext);

  useEffect(() => {
    if (!process.env.KIRO_TEST_MODE || !appStore) return;

    const socketPath = process.env.KIRO_TEST_TUI_IPC_SOCKET_PATH!;
    const socket = net.createConnection(socketPath);
    const connection = new TuiIpcConnection(socket);

    const handleCommands = async () => {
      for await (const command of connection.incomingCommands()) {
        const response = handleCommand(command.data);
        connection.sendResponse(command.id, response);
      }
    };

    handleCommands();

    const handleCommand = (command: TestCommand): TestResponse => {
      switch (command.kind) {
        case 'GET_STORE':
          // Maps don't survive JSON serialization — convert each to a plain
          // object. TS can't relate the resulting object literal to the
          // SerializedAppState mapped type, so assert it at this one boundary.
          // eslint-disable-next-line no-case-declarations
          const state = appStore.getState();
          return {
            kind: 'GET_STORE',
            data: {
              ...state,
              liveOutputs: Object.fromEntries(state.liveOutputs),
              pendingOAuthServers: Object.fromEntries(
                state.pendingOAuthServers
              ),
              sessions: Object.fromEntries(state.sessions),
              sessionMessages: Object.fromEntries(state.sessionMessages),
            } as unknown as SerializedAppState,
          };

        case 'MOCK_ERROR':
          // Handle error injection if needed
          return {
            kind: 'MOCK_ERROR',
          };

        case 'HEAP_SNAPSHOT': {
          try {
            const snapshot = Bun.generateHeapSnapshot();
            fs.writeFileSync(command.filename, JSON.stringify(snapshot));
            return {
              kind: 'HEAP_SNAPSHOT',
              filename: command.filename,
            };
          } catch (e) {
            return {
              kind: 'ERROR',
              error: `Heap snapshot failed: ${e}`,
            };
          }
        }

        case 'MEMORY_USAGE': {
          const mem = process.memoryUsage();
          return {
            kind: 'MEMORY_USAGE',
            data: {
              rss: mem.rss,
              heapUsed: mem.heapUsed,
              heapTotal: mem.heapTotal,
              external: mem.external,
              arrayBuffers: mem.arrayBuffers,
            },
          };
        }

        case 'FORCE_GC': {
          if (typeof Bun !== 'undefined') Bun.gc(true);
          return { kind: 'FORCE_GC' };
        }

        case 'MOCK_SESSION_UPDATE': {
          const mockClient = getMockSessionClient();
          if (!mockClient)
            return { kind: 'ERROR', error: 'Mock client not available' };
          mockClient.injectEvent(command.event);
          return { kind: 'MOCK_SESSION_UPDATE' };
        }

        case 'MOCK_ADD_SESSION': {
          // Test-only sideband to seed the lite subagent layout's
          // `sessions` map without orchestrating a real subagent_list_update
          // event. Subagent panel + kill-ladder tests need
          // sessions.values() to contain the stage row so
          // subagentSessionIdByName resolves the focused name.
          if (!appStore) {
            return { kind: 'ERROR', error: 'AppStoreContext not mounted' };
          }
          try {
            const now = new Date();
            const session = {
              agentName: command.session.name,
              status: 'busy' as const,
              type: 'ephemeral' as const,
              created: now,
              lastActivity: now,
              ...command.session,
            };
            appStore.getState().addSession(session as any);
            const after = appStore.getState().sessions;
            if (after.size === 0) {
              return {
                kind: 'ERROR',
                error: `addSession returned but sessions still empty (size=${after.size})`,
              };
            }
            return { kind: 'MOCK_ADD_SESSION' };
          } catch (e) {
            return {
              kind: 'ERROR',
              error: `MOCK_ADD_SESSION threw: ${(e as Error).message}`,
            };
          }
        }

        case 'MOCK_START_EDITING_QUEUE': {
          // Test-only sideband: the user-facing editing path goes
          // through the activity tray (Ctrl+X → ↑/↓ → Enter), which is
          // gated on tasks.length > 0 in lite mode. Driving it that way
          // would require seeding tasks unrelated to the assertion.
          // Calling startEditingQueue directly mirrors what the tray
          // does once the user picks a row.
          if (!appStore) {
            return { kind: 'ERROR', error: 'AppStoreContext not mounted' };
          }
          appStore.getState().startEditingQueue(command.index);
          return { kind: 'MOCK_START_EDITING_QUEUE' };
        }

        case 'COMPLETE_TURN': {
          const mockClient = getMockSessionClient();
          if (!mockClient)
            return { kind: 'ERROR', error: 'Mock client not available' };
          mockClient.completeTurn();
          return { kind: 'COMPLETE_TURN' };
        }

        default:
          throw new Error(`Unknown command: ${(command as TestCommand).kind}`);
      }
    };

    return () => {
      connection.close();
    };
  }, [appStore]);

  return <>{children}</>;
};
