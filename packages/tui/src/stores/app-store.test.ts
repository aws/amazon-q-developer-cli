import { describe, it, expect, mock } from 'bun:test';
import { createAppStore } from './app-store';
import { Kiro } from '../kiro';

// Mock Kiro
mock.module('../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    cancel: mock(),
    close: mock(),
  })),
}));

describe('AppStore input buffer', () => {
  it('backspace removes character at cursor', () => {
    const mockKiro = new Kiro();
    const store = createAppStore({ kiro: mockKiro });

    // Set up initial state with text
    store.getState().insert('h');
    store.getState().insert('i');

    // Verify initial state
    expect(store.getState().input.lines[0]).toBe('hi');
    expect(store.getState().input.cursorCol).toBe(2);

    // Test backspace
    store.getState().backspace();

    // Verify character was removed
    expect(store.getState().input.lines[0]).toBe('h');
    expect(store.getState().input.cursorCol).toBe(1);
  });

  it('delete removes character at cursor', () => {
    const mockKiro = new Kiro();
    const store = createAppStore({ kiro: mockKiro });

    store.getState().insert('h');
    store.getState().insert('i');
    // Position cursor at start
    const input = store.getState().input;
    store.setState({
      input: { ...input, cursorCol: 0, preferredCursorCol: 0 },
    });

    store.getState().delete();

    expect(store.getState().input.lines[0]).toBe('i');
    expect(store.getState().input.cursorCol).toBe(0);
  });

  it('delete merges with next line at end of line', () => {
    const mockKiro = new Kiro();
    const store = createAppStore({ kiro: mockKiro });

    store.getState().insert('a');
    store.getState().newline();
    store.getState().insert('b');
    // Position cursor at end of first line
    const input = store.getState().input;
    store.setState({
      input: { ...input, cursorRow: 0, cursorCol: 1, preferredCursorCol: 1 },
    });

    store.getState().delete();

    expect(store.getState().input.lines).toEqual(['ab']);
  });
});
