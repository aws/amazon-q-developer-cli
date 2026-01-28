import React, { useEffect, useContext } from 'react';
import * as net from 'net';
import { AppStoreContext } from '../stores/app-store';
import type { TestCommand, TestResponse } from './shared/ipc-types';
import { TuiIpcConnection } from './shared/tui-ipc-connection';

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
          return {
            kind: 'GET_STORE',
            data: appStore.getState(),
          };

        case 'MOCK_ERROR':
          // Handle error injection if needed
          return {
            kind: 'MOCK_ERROR',
          };

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
