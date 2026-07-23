import { afterEach, beforeEach, describe, expect, mock, test } from 'bun:test';
import { createAppStore } from '../app-store';

let terminalWrites: string[];
let originalWrite: typeof process.stdout.write;

beforeEach(() => {
  terminalWrites = [];
  originalWrite = process.stdout.write;
  process.stdout.write = ((chunk: string | Uint8Array) => {
    terminalWrites.push(String(chunk));
    return true;
  }) as typeof process.stdout.write;
});

afterEach(() => {
  process.stdout.write = originalWrite;
});

function makeStore() {
  const kiro = {
    sessionId: 'sess-1',
    close: mock(() => {}),
    isCloudSessionActive: () => false,
  };
  const store = createAppStore({ kiro: kiro as any, agentEngine: 'kas' });
  store.setState({ isInitialized: true });
  return store;
}

describe('handleUserInput `!` shell escape — cloud-session gate', () => {
  test('cloud: refuses before clear touches the terminal', async () => {
    const store = makeStore();
    store.setState({ cloudSessionActive: true });

    await store.getState().handleUserInput('!clear');

    expect(store.getState().transientAlert).toEqual({
      message: 'Shell commands are not available for a cloud session yet.',
      status: 'error',
      autoHideMs: 5000,
    });
    expect(terminalWrites).toEqual([]);
    expect(store.getState().messages).toHaveLength(0);
    expect(store.getState().isProcessing).toBe(false);
  });

  test('local: clear retains its terminal behavior', async () => {
    const store = makeStore();

    await store.getState().handleUserInput('!clear');

    expect(terminalWrites).toEqual(['\x1b[3J\x1b[2J\x1b[H']);
    expect(
      store.getState().transientAlert?.message.includes('cloud session') ??
        false
    ).toBe(false);
  });
});
