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
});
