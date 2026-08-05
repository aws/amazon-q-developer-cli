import { describe, test, expect, afterEach, vi } from 'vitest';
import React from 'react';
import { render, type Instance } from 'twinki';
import type { Terminal } from 'twinki';
import { createAppStore, AppStoreContext } from '../stores/app-store.js';
import { Kiro } from '../kiro.js';
import { ThemeContext } from '../theme/ThemeProvider.js';
import type { ThemeContextValue } from '../theme/ThemeProvider.js';

import { useColor } from './useColor.js';
import { useConversationContent } from './useConversationContent.js';
import { useExpandableOutput } from './useExpandableOutput.js';
import { useKeypress } from './useKeypress.js';
import { useKiro } from './useKiro.js';
import { useRenderMetrics } from './useRenderMetrics.js';
import { useScrollableBox } from './useScrollableBox.js';
import { useTerminalSize } from './useTerminalSize.js';
import { useTextStyle } from './useTextStyle.js';
import { useTheme } from './useThemeContext.js';
import { useBackendPanelHandlers } from '../components/layout/shared/useBackendPanelHandlers.js';

// ---------------------------------------------------------------------------
// MockTerminal -- minimal Terminal implementation for headless rendering
// ---------------------------------------------------------------------------
class MockTerminal implements Terminal {
  private _onInput: ((data: string) => void) | null = null;

  get columns() {
    return 80;
  }
  get rows() {
    return 24;
  }
  get kittyProtocolActive() {
    return true;
  }

  start(onInput: (data: string) => void, _onResize: () => void): void {
    this._onInput = onInput;
  }
  stop(): void {}
  async drainInput(): Promise<void> {}
  write(_data: string): void {}
  moveBy(_lines: number): void {}
  hideCursor(): void {}
  showCursor(): void {}
  clearLine(): void {}
  clearFromCursor(): void {}
  clearScreen(): void {}
  enableMouse(): void {}
  disableMouse(): void {}
  setTitle(_title: string): void {}

  sendInput(data: string): void {
    if (this._onInput) {
      this._onInput(data);
    }
  }
}

// ---------------------------------------------------------------------------
// Mock ThemeContext value
// ---------------------------------------------------------------------------
function createMockChalkChain() {
  const fn: any = (text: string) => text;
  fn.hex = '#ffffff';
  fn.bold = fn;
  fn.dim = fn;
  fn.italic = fn;
  fn.underline = fn;
  fn.strikethrough = fn;
  return fn;
}

const mockThemeContext: ThemeContextValue = {
  getColor: () => createMockChalkChain(),
  textStyles: {
    label: { color: 'primary' },
    selectedLabel: { color: 'primary' },
  },
  colors: {} as any,
  getUserPromptColor: () => createMockChalkChain(),
  getUserPromptBgHex: () => undefined,
  getUserResponseColor: () => createMockChalkChain(),
  setUserColors: () => {},
  setBaseTheme: () => {},
  baseTheme: {} as any,
  wrapDisabled: false,
} as any;

// ---------------------------------------------------------------------------
// renderHook -- renders a hook inside an AppStoreContext provider
// ---------------------------------------------------------------------------
let activeInstance: Instance | null = null;

afterEach(() => {
  if (activeInstance) {
    activeInstance.unmount();
    activeInstance = null;
  }
});

async function renderHook<T>(
  hook: () => T,
  storeOverrides?: Record<string, unknown>
): Promise<T> {
  const store = createAppStore({ kiro: new Kiro() });
  if (storeOverrides) {
    store.setState(storeOverrides as any);
  }

  return renderHookInStore(hook, store);
}

async function renderHookInStore<T>(
  hook: () => T,
  store: ReturnType<typeof createAppStore>
): Promise<T> {
  let captured: T | undefined;

  function TestComponent() {
    captured = hook();
    return null;
  }

  const Wrapper = () => (
    <AppStoreContext.Provider value={store}>
      <TestComponent />
    </AppStoreContext.Provider>
  );

  const instance = render(<Wrapper />, {
    terminal: new MockTerminal(),
    exitOnCtrlC: false,
  });
  activeInstance = instance;

  await new Promise((resolve) => setTimeout(resolve, 50));

  instance.unmount();
  activeInstance = null;

  return captured as T;
}

async function renderHookWithTheme<T>(hook: () => T): Promise<T> {
  const store = createAppStore({ kiro: new Kiro() });

  let captured: T | undefined;

  function TestComponent() {
    captured = hook();
    return null;
  }

  const Wrapper = () => (
    <AppStoreContext.Provider value={store}>
      <ThemeContext.Provider value={mockThemeContext}>
        <TestComponent />
      </ThemeContext.Provider>
    </AppStoreContext.Provider>
  );

  const instance = render(<Wrapper />, {
    terminal: new MockTerminal(),
    exitOnCtrlC: false,
  });
  activeInstance = instance;

  await new Promise((resolve) => setTimeout(resolve, 50));

  instance.unmount();
  activeInstance = null;

  return captured as T;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

describe('useColor', () => {
  test('returns a function with .hex property when called with no args', async () => {
    const result = await renderHook(() => useColor());
    expect(typeof result).toBe('function');
    expect(result).toHaveProperty('hex');
  });

  test('returns a function when called with a named color', async () => {
    const result = await renderHook(() =>
      useColor(undefined, undefined, 'red')
    );
    expect(typeof result).toBe('function');
    expect(result).toHaveProperty('hex');
  });
});

describe('useConversationContent', () => {
  test('returns expected shape with defaults', async () => {
    const result = await renderHook(() => useConversationContent());
    expect(result).toHaveProperty('isStreaming');
    expect(result).toHaveProperty('children');
    expect(result).toHaveProperty('startStreaming');
    expect(result).toHaveProperty('addChild');
    expect(result).toHaveProperty('addChildWithBarControl');
    expect(result).toHaveProperty('updateChild');
    expect(result).toHaveProperty('removeChild');
    expect(result).toHaveProperty('stopStreaming');
    expect(result.isStreaming).toBe(false);
    expect(result.children).toEqual([]);
  });

  test('startStreaming, addChild, stopStreaming are functions', async () => {
    const result = await renderHook(() => useConversationContent());
    expect(typeof result.startStreaming).toBe('function');
    expect(typeof result.addChild).toBe('function');
    expect(typeof result.stopStreaming).toBe('function');
  });
});

describe('useExpandableOutput', () => {
  test('returns expected shape with totalItems=10 previewCount=3', async () => {
    const result = await renderHook(() =>
      useExpandableOutput({ totalItems: 10, previewCount: 3 })
    );
    expect(result).toHaveProperty('expanded');
    expect(result).toHaveProperty('hasExpandableContent');
    expect(result).toHaveProperty('hiddenCount');
    expect(result).toHaveProperty('expandHint');
    expect(result.hasExpandableContent).toBe(true);
    expect(result.hiddenCount).toBe(7);
  });

  test('hasExpandableContent is false when totalItems <= previewCount', async () => {
    const result = await renderHook(() =>
      useExpandableOutput({ totalItems: 2, previewCount: 3 })
    );
    expect(result.hasExpandableContent).toBe(false);
    expect(result.hiddenCount).toBe(0);
  });

  test('tracks mounted expandable bodies as content changes', async () => {
    const store = createAppStore({ kiro: new Kiro() });
    let itemCounts = [10, 10];

    function TestComponent() {
      useExpandableOutput({ totalItems: itemCounts[0]!, previewCount: 3 });
      useExpandableOutput({ totalItems: itemCounts[1]!, previewCount: 3 });
      return null;
    }
    const renderTree = () => (
      <AppStoreContext.Provider value={store}>
        <TestComponent />
      </AppStoreContext.Provider>
    );
    const instance = render(renderTree(), {
      terminal: new MockTerminal(),
      exitOnCtrlC: false,
    });
    activeInstance = instance;
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(store.getState().hasExpandableToolOutputs).toBe(true);

    itemCounts = [1, 10];
    instance.rerender(renderTree());
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(store.getState().hasExpandableToolOutputs).toBe(true);

    itemCounts = [1, 1];
    instance.rerender(renderTree());
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(store.getState().hasExpandableToolOutputs).toBe(false);
  });
});

describe('useKeypress', () => {
  test('handler fires on terminal input', async () => {
    const store = createAppStore({ kiro: new Kiro() });
    const calls: Array<{ input: string }> = [];
    const terminal = new MockTerminal();

    function TestComponent() {
      useKeypress((input) => {
        calls.push({ input });
      });
      return null;
    }

    const Wrapper = () => (
      <AppStoreContext.Provider value={store}>
        <TestComponent />
      </AppStoreContext.Provider>
    );

    const instance = render(<Wrapper />, {
      terminal,
      exitOnCtrlC: false,
    });
    activeInstance = instance;

    await new Promise((resolve) => setTimeout(resolve, 50));

    terminal.sendInput('a');

    await new Promise((resolve) => setTimeout(resolve, 50));

    instance.unmount();
    activeInstance = null;

    expect(calls.length).toBeGreaterThan(0);
    expect(calls[0]!.input).toBe('a');
  });

  test('handler does not fire when isActive is false', async () => {
    const store = createAppStore({ kiro: new Kiro() });
    const calls: Array<{ input: string }> = [];
    const terminal = new MockTerminal();

    function TestComponent() {
      useKeypress(
        (input) => {
          calls.push({ input });
        },
        { isActive: false }
      );
      return null;
    }

    const Wrapper = () => (
      <AppStoreContext.Provider value={store}>
        <TestComponent />
      </AppStoreContext.Provider>
    );

    const instance = render(<Wrapper />, {
      terminal,
      exitOnCtrlC: false,
    });
    activeInstance = instance;

    await new Promise((resolve) => setTimeout(resolve, 50));

    terminal.sendInput('a');

    await new Promise((resolve) => setTimeout(resolve, 50));

    instance.unmount();
    activeInstance = null;

    expect(calls.length).toBe(0);
  });
});

describe('useKiro', () => {
  test('returns expected shape with defaults', async () => {
    const result = await renderHook(() => useKiro());
    expect(result).toHaveProperty('isProcessing');
    expect(result).toHaveProperty('error');
    expect(result).toHaveProperty('isReady');
    expect(result).toHaveProperty('sendMessage');
    expect(result).toHaveProperty('cancel');
    expect(result).toHaveProperty('setProcessing');
    expect(result).toHaveProperty('setError');
    expect(result.isProcessing).toBe(false);
    expect(result.error).toBeNull();
    expect(result.isReady).toBe(true);
  });
});

describe('useBackendPanelHandlers', () => {
  const freshBreakdown = {
    contextFiles: { percent: 7, tokens: 700 },
    tools: { percent: 1, tokens: 100 },
    kiroResponses: { percent: 2, tokens: 200 },
    yourPrompts: { percent: 3, tokens: 300 },
  };

  test('reads the KAS context cache when the captured handler runs', async () => {
    const executeCommand = vi.fn();
    const store = createAppStore({
      kiro: { executeCommand } as any,
      agentEngine: 'kas',
    });
    const handlers = await renderHookInStore(useBackendPanelHandlers, store);

    // The handler was captured before this update and cannot rerender.
    store.setState({ contextBreakdownCache: freshBreakdown });
    await handlers.handleTabFromUsage();

    expect(executeCommand).not.toHaveBeenCalled();
    expect(store.getState()).toMatchObject({
      showContextBreakdown: true,
      contextBreakdown: freshBreakdown,
      showUsagePanel: false,
    });
  });

  test('queries V2 even when the store contains a context cache', async () => {
    const backendBreakdown = {
      ...freshBreakdown,
      contextFiles: { percent: 9, tokens: 900 },
    };
    const executeCommand = vi.fn().mockResolvedValue({
      data: { breakdown: backendBreakdown },
    });
    const store = createAppStore({
      kiro: { executeCommand } as any,
      agentEngine: 'v2',
    });
    store.setState({ contextBreakdownCache: freshBreakdown });
    const handlers = await renderHookInStore(useBackendPanelHandlers, store);

    await handlers.handleTabFromUsage();

    expect(executeCommand).toHaveBeenCalledWith({
      command: 'context',
      args: {},
    });
    expect(store.getState().contextBreakdown).toBe(backendBreakdown);
  });

  test('uses a KAS cache update received while contextShow is pending', async () => {
    let resolveCommand!: (result: {
      success: boolean;
      message: string;
    }) => void;
    const commandResult = new Promise<{ success: boolean; message: string }>(
      (resolve) => {
        resolveCommand = resolve;
      }
    );
    const executeCommand = vi.fn(() => commandResult);
    const store = createAppStore({
      kiro: { executeCommand } as any,
      agentEngine: 'kas',
    });
    const handlers = await renderHookInStore(useBackendPanelHandlers, store);

    const handling = handlers.handleTabFromUsage();
    store.setState({ contextBreakdownCache: freshBreakdown });
    resolveCommand({ success: true, message: '' });
    await handling;

    expect(store.getState().contextBreakdown).toBe(freshBreakdown);
  });

  test('drains only after a rewind selection dispatch completes', async () => {
    const order: string[] = [];
    const store = createAppStore({ kiro: {} as any });
    store.setState({
      activeCommand: {
        command: { name: '/rewind', description: 'Rewind' },
        options: [],
      },
      handleUserInput: vi.fn(async () => {
        order.push('dispatch');
      }),
      processQueue: vi.fn(async () => {
        order.push('drain');
      }),
    });
    const handlers = await renderHookInStore(useBackendPanelHandlers, store);

    await handlers.handleRewindSelect('turn-1');

    expect(order).toEqual(['dispatch', 'drain']);
    expect(store.getState().activeCommand).toBeNull();
  });

  test('drains only after a tangent selection dispatch completes', async () => {
    const order: string[] = [];
    const store = createAppStore({
      kiro: { sessionId: 'current' } as any,
    });
    store.setState({
      activeCommand: {
        command: { name: '/tangent', description: 'Tangent' },
        options: [],
      },
      dispatchSlashCommand: vi.fn(async () => {
        order.push('dispatch');
      }),
      processQueue: vi.fn(async () => {
        order.push('drain');
      }),
    });
    const handlers = await renderHookInStore(useBackendPanelHandlers, store);

    await handlers.handleTangentSelect('next', 'Next');

    expect(order).toEqual(['dispatch', 'drain']);
    expect(store.getState().activeCommand).toBeNull();
  });
});

describe('useRenderMetrics', () => {
  test('returns null when no __TWINKI_INSTANCE__ on globalThis', async () => {
    const result = await renderHook(() => useRenderMetrics());
    expect(result).toBeNull();
  });
});

describe('useScrollableBox', () => {
  test('returns expected shape with contentHeight=100 viewHeight=20', async () => {
    const result = await renderHook(() =>
      useScrollableBox({ contentHeight: 100, viewHeight: 20 })
    );
    expect(result).toHaveProperty('scrollTop');
    expect(result).toHaveProperty('maxScroll');
    expect(result).toHaveProperty('scroll');
    expect(result).toHaveProperty('scrollToBottom');
    expect(result).toHaveProperty('onKey');
    expect(result).toHaveProperty('thumbSize');
    expect(result).toHaveProperty('thumbTop');
    expect(result.scrollTop).toBe(0);
    expect(result.maxScroll).toBe(80);
  });
});

describe('useTerminalSize', () => {
  test('returns an object with numeric width and height', async () => {
    const result = await renderHook(() => useTerminalSize());
    expect(typeof result.width).toBe('number');
    expect(typeof result.height).toBe('number');
    expect(result.width).toBeGreaterThan(0);
    expect(result.height).toBeGreaterThan(0);
  });
});

describe('useTextStyle', () => {
  test('returns a function (chalk chain) for a valid style name', async () => {
    const result = await renderHookWithTheme(() => useTextStyle('label'));
    expect(typeof result).toBe('function');
  });
});

describe('useThemeContext', () => {
  test('returns an object with getColor function', async () => {
    const result = await renderHookWithTheme(() => useTheme());
    expect(result).toHaveProperty('getColor');
    expect(typeof result.getColor).toBe('function');
  });
});
