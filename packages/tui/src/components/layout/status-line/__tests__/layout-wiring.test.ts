/**
 * Proves each real layout hands billing figures to its status surface.
 *
 * The surfaces are pure views, so the fetch lives in the layout and reaches the
 * bar as props. Nothing else asserts that hand-off: surface tests supply the
 * numbers directly, and the PTY tests run against a session with no usage data.
 */
import {
  describe,
  it,
  expect,
  afterEach,
  afterAll,
  beforeEach,
} from 'bun:test';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';
import { mkdtempSync, rmSync, mkdirSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

const tempHome = mkdtempSync(join(tmpdir(), 'kiro-status-wiring-test-'));
const originalHome = process.env.KIRO_HOME;
// Set per test rather than here: a module-scope assignment outlives this file
// and would point other suites at these fixtures.
mkdirSync(join(tempHome, 'settings'), { recursive: true });
writeFileSync(
  join(tempHome, 'settings', 'cli.json'),
  JSON.stringify({
    'chat.statusLine.tui': { usage: true, credits: true },
    'chat.statusLine.lite': { usage: true, credits: true },
  })
);

const { UI_VARIANTS } = await import('../../ui-variants.js');
const { AppStoreContext, createAppStore } =
  await import('../../../../stores/app-store.js');
const { Kiro } = await import('../../../../kiro.js');
type UiMode = import('../../../../types/ui-mode.js').UiMode;
type StatusSurfaceProps = import('../../status-surface.js').StatusSurfaceProps;
type VariantLayoutProps = import('../../variant-layout.js').VariantLayoutProps;

class MockTerminal implements Terminal {
  public output = '';
  get columns() {
    return 120;
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
}

const { invalidateStatusSegments } =
  await import('../../status-line/config.js');
let active: Instance | null = null;

beforeEach(() => {
  process.env.KIRO_HOME = tempHome;
  invalidateStatusSegments();
});

afterEach(() => {
  if (originalHome === undefined) delete process.env.KIRO_HOME;
  else process.env.KIRO_HOME = originalHome;
  active?.unmount();
  active = null;
});

afterAll(() => {
  if (originalHome === undefined) delete process.env.KIRO_HOME;
  else process.env.KIRO_HOME = originalHome;
  rmSync(tempHome, { recursive: true, force: true });
});

/** A kiro whose `usage` command reports a known limited allowance. */
function kiroReportingUsage(used: number, limit: number) {
  const kiro = new Kiro();
  (kiro as unknown as { executeCommand: unknown }).executeCommand =
    async () => ({
      success: true,
      data: { usageBreakdowns: [{ hasLimit: true, limit, used }] },
    });
  return kiro;
}

const flush = () => new Promise((r) => setTimeout(r, 120));

describe('status line billing wiring', () => {
  it('forwards billing figures through each layout status surface', async () => {
    for (const [variant, { Layout, ...surfaces }] of Object.entries(
      UI_VARIANTS
    ) as Array<[UiMode, { Layout: React.FC<VariantLayoutProps> }]>) {
      let statusProps: StatusSurfaceProps | undefined;
      const injected = Object.fromEntries(
        Object.keys(surfaces).map((name) => [name, () => null])
      ) as unknown as VariantLayoutProps;
      injected.StatusLine = (props) => {
        statusProps = props;
        return null;
      };

      const store = createAppStore({
        kiro: kiroReportingUsage(2040, 10000),
        agentEngine: 'v2',
      });
      store.setState({
        uiMode: variant,
        mode: 'inline',
        sessionId: 'session-1',
        isInitialized: true,
        lite: { ...store.getState().lite, scrollbackClearToken: -1 },
      });

      active = render(
        React.createElement(
          AppStoreContext.Provider,
          { value: store },
          React.createElement(Layout, injected)
        ),
        { terminal: new MockTerminal(), exitOnCtrlC: false }
      );
      await flush();

      expect({ variant, ...statusProps }).toMatchObject({
        variant,
        usagePercent: 20,
        creditsRemaining: 7960,
      });

      active.unmount();
      active = null;
    }
  });
});
