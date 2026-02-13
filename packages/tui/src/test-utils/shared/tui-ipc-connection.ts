/**
 * IPC connection for accessing TUI state during test scenarios.
 * Handles bidirectional communication over newline-delimited JSON.
 */

import * as net from 'net';
import type {
  TestCommand,
  TestResponse,
  TestMessage,
  TestMessageCommand,
  TestMessageResponse,
} from './ipc-types';

/**
 * Represents a connection to a running TUI process for test scenarios.
 * Enables accessing and updating TUI state via IPC using newline-delimited JSON.
 */
export class TuiIpcConnection {
  private buffer = '';
  private socket: net.Socket;
  private commandHandlers: Set<(command: TestMessageCommand) => void>;
  private responseHandlers: Set<(response: TestMessageResponse) => void>;

  constructor(socket: net.Socket) {
    this.socket = socket;
    this.commandHandlers = new Set();
    this.responseHandlers = new Set();
    this.setupDataHandler();
  }

  onCommand(handler: (command: TestMessageCommand) => void) {
    this.commandHandlers.add(handler);
    return () => this.commandHandlers.delete(handler);
  }

  onResponse(handler: (response: TestMessageResponse) => void) {
    this.responseHandlers.add(handler);
    return () => this.responseHandlers.delete(handler);
  }

  /**
   * Sends a command over the socket.
   */
  sendCommand(command: TestCommand): Promise<TestMessageResponse> {
    const id = Math.floor(Math.random() * 1_000_000).toString();

    let cleanup: () => void;
    const promise = new Promise<TestMessageResponse>((resolve, reject) => {
      cleanup = this.onResponse((response) => {
        if (response.id !== id) return;
        if (response.data.kind !== command.kind) {
          reject(
            new Error(
              `Received unexpected response: ${JSON.stringify(response)}`
            )
          );
        } else {
          resolve(response);
        }
      });
    });

    const message: TestMessageCommand = { id, kind: 'command', data: command };
    this.socket.write(JSON.stringify(message) + '\n');

    return promise.finally(() => cleanup());
  }

  /**
   * Sends a response over the socket.
   */
  sendResponse(id: string, response: TestResponse): void {
    const message: TestMessage = { id, kind: 'response', data: response };
    this.socket.write(JSON.stringify(message) + '\n');
  }

  /**
   * Yields incoming commands as an async generator.
   */
  async *incomingCommands(): AsyncGenerator<TestMessageCommand> {
    const queue: TestMessageCommand[] = [];
    let resolve: (() => void) | null = null;

    const cleanup = this.onCommand((command) => {
      queue.push(command);
      resolve?.();
    });

    try {
      while (true) {
        while (queue.length > 0) {
          yield queue.shift()!;
        }
        await new Promise<void>((r) => {
          resolve = r;
        });
        resolve = null;
      }
    } finally {
      cleanup();
    }
  }

  /**
   * Sets up data handling for incoming messages.
   */
  private setupDataHandler(): void {
    this.socket.on('data', (data: Buffer) => {
      this.buffer += data.toString();

      // Process complete lines
      const lines = this.buffer.split('\n');
      this.buffer = lines.pop() || ''; // Keep incomplete line in buffer

      for (const line of lines) {
        if (line.trim()) {
          try {
            const message: TestMessage = JSON.parse(line);
            switch (message.kind) {
              case 'command': {
                for (const handler of this.commandHandlers) {
                  handler(message);
                }
                break;
              }
              case 'response': {
                for (const handler of this.responseHandlers) {
                  handler(message);
                }
              }
            }
          } catch (error) {
            throw new Error(
              `Failed to parse JSON line: ${error}. Line: ${line}`,
              { cause: error }
            );
          }
        }
      }
    });
  }

  /**
   * Closes the connection.
   */
  close(): void {
    this.socket.end();
  }
}
