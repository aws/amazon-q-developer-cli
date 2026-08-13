import { afterEach, describe, expect, test } from 'vitest';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';
import { ThinkingMessage } from './ThinkingMessage.js';
import { kiroDark } from '../../../theme/kiroDark.js';
import { AppStoreContext, createAppStore } from '../../../stores/app-store.js';
import { Kiro } from '../../../kiro.js';

class MockTerminal implements Terminal {
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
  sendInput(): void {}
}

let activeInstance: Instance | null = null;

afterEach(() => {
  activeInstance?.unmount();
  activeInstance = null;
});

async function flush(): Promise<void> {
  await Promise.resolve();
  await new Promise((resolve) => setTimeout(resolve, 20));
  await Promise.resolve();
}

function mount(props: React.ComponentProps<typeof ThinkingMessage>) {
  const terminal = new MockTerminal();
  const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
  activeInstance = render(
    <AppStoreContext.Provider value={store}>
      <ThinkingMessage {...props} />
    </AppStoreContext.Provider>,
    { terminal }
  );
  return terminal;
}

function ansiFgFor(hex: string): string {
  const clean = hex.replace('#', '');
  const r = parseInt(clean.slice(0, 2), 16);
  const g = parseInt(clean.slice(2, 4), 16);
  const b = parseInt(clean.slice(4, 6), 16);
  return `\x1b[38;2;${r};${g};${b}m`;
}

// The brand and secondary truecolor hexes come from the default theme
// context value (kiroDark), which useTheme() falls back to outside a
// <ThemeProvider>. Asserting against the theme's own hexes keeps this test
// correct even if the palette values change later.
const brandHex = kiroDark.colors.brand.truecolor!;
const secondaryHex = kiroDark.colors.secondary.truecolor!;

describe('ThinkingMessage title color', () => {
  test('"Thinking..." title is painted with the brand color, not secondary', async () => {
    const terminal = mount({});
    await flush();
    expect(terminal.output).toContain(ansiFgFor(brandHex));
    expect(terminal.output).not.toContain(ansiFgFor(secondaryHex));
    expect(terminal.output).toContain('Thinking...');
  });

  test("title matches a non-default agent's bar color instead of brand", async () => {
    // e.g. the "spec" agent's hashed color from getAgentColor — the spinner
    // (via StatusBar's barColor) and the title must match, not diverge.
    const agentHex = '#af8700';
    const terminal = mount({ barColor: agentHex });
    await flush();
    expect(terminal.output).toContain(ansiFgFor(agentHex));
    // Must NOT fall back to the brand purple when a bar color is supplied.
    expect(terminal.output).not.toContain(ansiFgFor(brandHex));
  });
});
