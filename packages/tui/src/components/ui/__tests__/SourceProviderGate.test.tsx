import { afterEach, describe, expect, test, vi } from 'vitest';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';
import stripAnsi from 'strip-ansi';
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
  isRemote?: boolean;
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
      isRemote={overrides?.isRemote ?? false}
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

  test('local device: no remote guidance (browser option works)', async () => {
    const h = mountGate();
    await flush();
    expect(h.terminal.output).not.toContain('Open the URL above');
  });

  test('remote device: renders the guidance under the URL', async () => {
    const h = mountGate({ isRemote: true });
    await flush();
    // The hint wraps across terminal lines and carries ANSI styling — strip
    // both and collapse whitespace before asserting on the sentence.
    const flat = stripAnsi(h.terminal.output).replace(/\s+/g, ' ');
    expect(flat).toContain(
      "Open the URL above on any device where you're signed in, connect a " +
        "source provider, then select 'Refresh and try again' below."
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

  describe('remote/headless device (isRemote)', () => {
    test('omits the "Open in browser" option entirely', async () => {
      const h = mountGate({ isRemote: true });
      await flush();
      expect(h.terminal.output).not.toContain('Open in browser');
      expect(h.terminal.output).toContain('Refresh and try again');
      expect(h.terminal.output).toContain('Quit');
    });

    test('cursor starts on "Refresh and try again" and Enter retries', async () => {
      const h = mountGate({ isRemote: true });
      await flush();
      h.terminal.sendInput(ENTER);
      await flush();
      expect(h.onRetry).toHaveBeenCalledTimes(1);
      expect(h.onOpenBrowser).not.toHaveBeenCalled();
    });

    test('up-arrow clamps at the top; the browser handler is unreachable', async () => {
      const h = mountGate({ isRemote: true });
      await flush();
      h.terminal.sendInput(UP);
      h.terminal.sendInput(UP);
      await flush();
      h.terminal.sendInput(ENTER);
      await flush();
      expect(h.onRetry).toHaveBeenCalledTimes(1);
      expect(h.onOpenBrowser).not.toHaveBeenCalled();
    });

    test('navigation still reaches Quit below', async () => {
      const h = mountGate({ isRemote: true });
      await flush();
      h.terminal.sendInput(DOWN);
      await flush();
      h.terminal.sendInput(ENTER);
      await flush();
      expect(h.onQuit).toHaveBeenCalledTimes(1);
    });
  });
});
