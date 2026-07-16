import { afterEach, describe, expect, test, vi } from 'vitest';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';
import { SourceProviderGate } from '../SourceProviderGate.js';

const DOWN = '\x1b[B';
const UP = '\x1b[A';
const ENTER = '\r';

class MockTerminal implements Terminal {
  private onInput: ((data: string) => void) | null = null;
  public output = '';
  get columns() {
    return 80;
  }
  get rows() {
    return 24;
  }
  get kittyProtocolActive() {
    return true;
  }
  start(onInput: (data: string) => void): void {
    this.onInput = onInput;
  }
  stop(): void {}
  async drainInput(): Promise<void> {}
  write(data: string): void {
    this.output += data;
  }
  moveBy(): void {}
  hideCursor(): void {}
  showCursor(): void {}
  clearLine(): void {}
  clearFromCursor(): void {}
  clearScreen(): void {}
  enableMouse(): void {}
  disableMouse(): void {}
  setTitle(): void {}
  sendInput(data: string): void {
    this.onInput?.(data);
  }
}

let activeInstance: Instance | null = null;

afterEach(() => {
  activeInstance?.unmount();
  activeInstance = null;
});

async function flush(): Promise<void> {
  for (let i = 0; i < 3; i += 1) {
    await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 50));
  }
}

function mountGate(overrides?: {
  onOpenBrowser?: () => void;
  onRetry?: () => Promise<void> | void;
  onQuit?: () => void;
}) {
  const onOpenBrowser = overrides?.onOpenBrowser ?? vi.fn();
  const onRetry = overrides?.onRetry ?? vi.fn();
  const onQuit = overrides?.onQuit ?? vi.fn();
  const terminal = new MockTerminal();
  activeInstance = render(
    <SourceProviderGate
      setupUrl="https://kiro.dev/settings/source-providers"
      onOpenBrowser={onOpenBrowser}
      onRetry={onRetry}
      onQuit={onQuit}
    />,
    { terminal, exitOnCtrlC: false }
  );
  return { terminal, onOpenBrowser, onRetry, onQuit };
}

describe('SourceProviderGate', () => {
  test('renders the message and the setup URL', async () => {
    const h = mountGate();
    await flush();
    expect(h.terminal.output).toContain('Source provider not found');
    expect(h.terminal.output).toContain(
      'https://kiro.dev/settings/source-providers'
    );
  });

  test('Enter on the first option opens the browser', async () => {
    const h = mountGate();
    await flush();
    h.terminal.sendInput(ENTER);
    await flush();
    expect(h.onOpenBrowser).toHaveBeenCalledTimes(1);
    expect(h.onRetry).not.toHaveBeenCalled();
    expect(h.onQuit).not.toHaveBeenCalled();
  });

  test('down then Enter re-runs the connection check', async () => {
    const h = mountGate();
    await flush();
    h.terminal.sendInput(DOWN);
    await flush();
    h.terminal.sendInput(ENTER);
    await flush();
    expect(h.onRetry).toHaveBeenCalledTimes(1);
    expect(h.onQuit).not.toHaveBeenCalled();
  });

  test('navigating to the last option and Enter quits', async () => {
    const h = mountGate();
    await flush();
    h.terminal.sendInput(DOWN);
    h.terminal.sendInput(DOWN);
    await flush();
    h.terminal.sendInput(ENTER);
    await flush();
    expect(h.onQuit).toHaveBeenCalledTimes(1);
  });

  test('cursor clamps at the top with repeated up-arrows', async () => {
    const h = mountGate();
    await flush();
    h.terminal.sendInput(UP);
    h.terminal.sendInput(UP);
    await flush();
    h.terminal.sendInput(ENTER);
    await flush();
    // Still on the first option after clamping — opens the browser.
    expect(h.onOpenBrowser).toHaveBeenCalledTimes(1);
  });
});
