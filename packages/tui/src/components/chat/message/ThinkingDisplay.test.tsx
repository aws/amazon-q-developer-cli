import { afterEach, describe, expect, test } from 'vitest';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';
import { ThinkingDisplay } from './ThinkingDisplay.js';
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

function mount(props: React.ComponentProps<typeof ThinkingDisplay>) {
  const terminal = new MockTerminal();
  const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
  activeInstance = render(
    <AppStoreContext.Provider value={store}>
      <ThinkingDisplay {...props} />
    </AppStoreContext.Provider>,
    { terminal }
  );
  return terminal;
}

// The brand and secondary truecolor hexes come from the default theme
// context value (kiroDark), which useTheme() falls back to outside a
// <ThemeProvider>. Asserting against the theme's own hexes keeps this test
// correct even if the palette values change later.
const brandHex = kiroDark.colors.brand.truecolor!.replace('#', '');
const secondaryHex = kiroDark.colors.secondary.truecolor!.replace('#', '');

describe('ThinkingDisplay title color', () => {
  test('live "Thinking..." title is painted with the brand color', async () => {
    const terminal = mount({ text: 'reasoning about the fix' });
    await flush();
    // ANSI truecolor foreground escape: ESC[38;2;r;g;b m
    const r = parseInt(brandHex.slice(0, 2), 16);
    const g = parseInt(brandHex.slice(2, 4), 16);
    const b = parseInt(brandHex.slice(4, 6), 16);
    expect(terminal.output).toContain(`\x1b[38;2;${r};${g};${b}m`);
    expect(terminal.output).toContain('Thinking...');
  });

  test('completed "Thought for Ns..." title stays the secondary (dim) color, not brand', async () => {
    const terminal = mount({
      text: 'reasoning about the fix',
      thinkingMs: 3200,
    });
    await flush();
    const r = parseInt(secondaryHex.slice(0, 2), 16);
    const g = parseInt(secondaryHex.slice(2, 4), 16);
    const b = parseInt(secondaryHex.slice(4, 6), 16);
    expect(terminal.output).toContain(`\x1b[38;2;${r};${g};${b}m`);
    expect(terminal.output).toContain('Thought for 4s...');
  });

  test("live title matches a non-default agent's bar color instead of brand", async () => {
    // e.g. the "spec" agent's hashed color from getAgentColor — the spinner
    // (via StatusBar's barColor) and the title must match, not diverge.
    const agentHex = 'af8700';
    const terminal = mount({
      text: 'reasoning about the fix',
      barColor: `#${agentHex}`,
    });
    await flush();
    const r = parseInt(agentHex.slice(0, 2), 16);
    const g = parseInt(agentHex.slice(2, 4), 16);
    const b = parseInt(agentHex.slice(4, 6), 16);
    expect(terminal.output).toContain(`\x1b[38;2;${r};${g};${b}m`);
    // Must NOT fall back to the brand purple when a bar color is supplied.
    const brandHexBytes = [
      parseInt(brandHex.slice(0, 2), 16),
      parseInt(brandHex.slice(2, 4), 16),
      parseInt(brandHex.slice(4, 6), 16),
    ];
    expect(terminal.output).not.toContain(
      `\x1b[38;2;${brandHexBytes[0]};${brandHexBytes[1]};${brandHexBytes[2]}m`
    );
  });
});
