import { afterEach } from 'vitest';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';
import stripAnsi from 'strip-ansi';
import {
  AppStoreContext,
  createAppStore,
} from '../../../../stores/app-store.js';
import { ThemeProvider } from '../../../../theme/ThemeProvider.js';
import { GlyphsProvider } from '../../../../hooks/useGlyphs.js';
import { Kiro } from '../../../../kiro.js';

/**
 * Shared Twinki render harness for the inline tool-verbosity render tests.
 * Hosts the MockTerminal + flush + auto-unmount plumbing that every render
 * test needs so the test files only declare element + assertions.
 */
class MockTerminal implements Terminal {
  public output = '';
  constructor(
    public readonly columns: number,
    public readonly rows: number
  ) {}
  get kittyProtocolActive() {
    return true;
  }
  start(): void {}
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
}

let activeInstance: Instance | null = null;

afterEach(() => {
  activeInstance?.unmount();
  activeInstance = null;
});

async function flush(): Promise<void> {
  for (let i = 0; i < 3; i += 1) {
    await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 20));
  }
}

/** Render `element`, flush async paints, and return the RAW output with ANSI
 *  escapes intact — for assertions about color/reset sequences (e.g. no color
 *  bleed on clip). */
export async function renderRaw(
  element: React.ReactElement,
  { columns = 100, rows = 40 }: { columns?: number; rows?: number } = {}
): Promise<string> {
  const terminal = new MockTerminal(columns, rows);
  activeInstance = render(element, { terminal, exitOnCtrlC: false });
  await flush();
  return terminal.output;
}

/** Like {@link renderRaw} but returns the plain (ANSI-stripped) output. */
export async function renderPlain(
  element: React.ReactElement,
  opts?: { columns?: number; rows?: number }
): Promise<string> {
  return stripAnsi(await renderRaw(element, opts));
}

/** Render `node` wrapped in the real AppStore + Theme + Glyphs providers (the
 *  live-TUI dispatcher path), 120x40. `store` overrides feed createAppStore
 *  (e.g. uiMode / agentEngine). */
export function renderWithProviders(
  node: React.ReactElement,
  {
    store,
    configureStore,
    ...opts
  }: {
    columns?: number;
    rows?: number;
    store?: Omit<Parameters<typeof createAppStore>[0], 'kiro'>;
    configureStore?: (store: ReturnType<typeof createAppStore>) => void;
  } = { columns: 120, rows: 40 }
): Promise<string> {
  const appStore = createAppStore({ kiro: new Kiro(), ...store });
  configureStore?.(appStore);
  return renderPlain(
    <AppStoreContext.Provider value={appStore}>
      <ThemeProvider>
        <GlyphsProvider>{node}</GlyphsProvider>
      </ThemeProvider>
    </AppStoreContext.Provider>,
    opts
  );
}
