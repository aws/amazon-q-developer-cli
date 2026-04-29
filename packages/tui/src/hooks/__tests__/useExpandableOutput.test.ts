import { describe, it, expect, mock, afterAll } from 'bun:test';
import { createAppStore } from '../../stores/app-store';
import { Kiro } from '../../kiro';
import { Settings } from '../../constants/settings';

mock.module('../../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    cancel: mock(),
    close: mock(),
  })),
}));

afterAll(() => {
  mock.restore();
});

function createTestStore() {
  const mockKiro = new Kiro();
  return createAppStore({ kiro: mockKiro });
}

describe('auto-expand tool output setting', () => {
  it('toolOutputsExpanded defaults to false', () => {
    const store = createTestStore();
    expect(store.getState().toolOutputsExpanded).toBe(false);
  });

  it('setting is read from settings map', () => {
    const store = createTestStore();
    store.setState({
      settings: { [Settings.CHAT_AUTO_EXPAND_TOOL_OUTPUT]: true },
    });
    const settings = store.getState().settings;
    expect(settings?.[Settings.CHAT_AUTO_EXPAND_TOOL_OUTPUT]).toBe(true);
  });

  it('setting defaults to false when not present', () => {
    const store = createTestStore();
    store.setState({ settings: {} });
    const settings = store.getState().settings;
    expect(settings?.[Settings.CHAT_AUTO_EXPAND_TOOL_OUTPUT]).toBeUndefined();
  });

  it('setting coexists with other settings', () => {
    const store = createTestStore();
    store.setState({
      settings: {
        [Settings.CHAT_GREETING_ENABLED]: true,
        [Settings.CHAT_AUTO_EXPAND_TOOL_OUTPUT]: true,
      },
    });
    const settings = store.getState().settings;
    expect(settings?.[Settings.CHAT_GREETING_ENABLED]).toBe(true);
    expect(settings?.[Settings.CHAT_AUTO_EXPAND_TOOL_OUTPUT]).toBe(true);
  });

  it('toggleToolOutputsExpanded still works independently of setting', () => {
    const store = createTestStore();
    store.setState({
      settings: { [Settings.CHAT_AUTO_EXPAND_TOOL_OUTPUT]: true },
    });

    // Manual toggle should still function
    expect(store.getState().toolOutputsExpanded).toBe(false);
    store.getState().toggleToolOutputsExpanded();
    expect(store.getState().toolOutputsExpanded).toBe(true);
    store.getState().toggleToolOutputsExpanded();
    expect(store.getState().toolOutputsExpanded).toBe(false);
  });
});
