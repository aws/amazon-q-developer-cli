/**
 * Unit tests for the status-line settings panel.
 *
 * The panel is the only way to change the bar, so what matters is that it offers
 * every segment, states each one's default for the surface being edited, and
 * writes only the differences from that default.
 */
import {
  describe,
  it,
  expect,
  afterEach,
  afterAll,
  beforeEach,
  mock,
} from 'bun:test';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';
import stripAnsi from 'strip-ansi';

// The panel sizes its window from the terminal height, so drive that directly
// rather than reaching into the stream and leaving it changed for other suites.
const termSize = { width: 200, height: 120 };
// mock.module is process-global and survives this file — snapshot the real
// modules and re-register them afterAll so mocks cannot leak into other files.
import { restoreRealModulesAfterAll } from '../../../test-utils/restore-modules.js';

restoreRealModulesAfterAll(import.meta.dir, [
  '../../../hooks/useTerminalSize.js',
]);

mock.module('../../../hooks/useTerminalSize.js', () => ({
  useTerminalSize: () => ({ ...termSize }),
}));
import {
  mkdtempSync,
  rmSync,
  mkdirSync,
  writeFileSync,
  readFileSync,
} from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

const tempHome = mkdtempSync(join(tmpdir(), 'kiro-statusline-panel-test-'));
const originalHome = process.env.KIRO_HOME;
// Set per test rather than here: a module-scope assignment outlives this file
// and would point other suites at these fixtures.
mkdirSync(join(tempHome, 'settings'), { recursive: true });

const { StatusLineSettingsPanel } =
  await import('../StatusLineSettingsPanel.js');
const { AppStoreContext, createAppStore } =
  await import('../../../stores/app-store.js');
const { Kiro } = await import('../../../kiro.js');
const { STATUS_SEGMENT_LABELS } =
  await import('../../layout/status-line/labels.js');
const { STATUS_SEGMENT_IDS } =
  await import('../../layout/status-line/segments.js');

class MockTerminal implements Terminal {
  public output = '';
  constructor(private readonly width = 200) {}
  get columns() {
    return this.width;
  }
  get rows() {
    return 40;
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
  await import('../../layout/status-line/config.js');
let active: Instance | null = null;

beforeEach(() => {
  process.env.KIRO_HOME = tempHome;
  // Tall enough that the window shows every row; the short-pane case sets its own.
  termSize.height = 120;
  invalidateStatusSegments();
  writeFileSync(join(tempHome, 'settings', 'cli.json'), '{}');
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

async function paint(surface: 'tui' | 'lite'): Promise<string> {
  const terminal = new MockTerminal();
  const store = createAppStore({ kiro: new Kiro(), agentEngine: 'v2' });
  active = render(
    React.createElement(
      AppStoreContext.Provider,
      { value: store },
      React.createElement(StatusLineSettingsPanel, {
        surface,
        onClose: () => {},
      })
    ),
    { terminal, exitOnCtrlC: false }
  );
  await new Promise((r) => setTimeout(r, 80));
  return stripAnsi(terminal.output);
}

function saved(surface: 'tui' | 'lite'): unknown {
  const raw = readFileSync(join(tempHome, 'settings', 'cli.json'), 'utf-8');
  return JSON.parse(raw)[`chat.statusLine.${surface}`];
}

describe('StatusLineSettingsPanel', () => {
  it('offers every segment plus a reset row', async () => {
    const out = await paint('tui');
    for (const id of STATUS_SEGMENT_IDS) {
      expect(out).toContain(STATUS_SEGMENT_LABELS[id].label);
    }
    expect(out).toContain('Reset to defaults');
  });

  it('groups rows by the edited surface own defaults', async () => {
    // Date ships off on both surfaces; Agent ships on. The group a row lands in
    // has to follow that rather than a fixed list.
    const out = await paint('tui');
    const agentAt = out.indexOf('Agent');
    const dateAt = out.indexOf('Date');
    expect(out.lastIndexOf('On by default', agentAt)).toBeGreaterThan(
      out.lastIndexOf('Off by default', agentAt)
    );
    expect(out.lastIndexOf('Off by default', dateAt)).toBeGreaterThan(
      out.lastIndexOf('On by default', dateAt)
    );
  });

  it('omits a segment the surface cannot paint', async () => {
    // Lite has no code-intelligence renderer, so offering the row would give the
    // user a toggle that changes nothing.
    expect(await paint('tui')).toContain('Code intelligence');
    active?.unmount();
    active = null;
    expect(await paint('lite')).not.toContain('Code intelligence');
  });

  it('names the surface being edited', async () => {
    expect(await paint('tui')).toContain('tui status line');
    active?.unmount();
    active = null;
    expect(await paint('lite')).toContain('lite status line');
  });

  it('shows each segment state, not just its default', async () => {
    writeFileSync(
      join(tempHome, 'settings', 'cli.json'),
      JSON.stringify({ 'chat.statusLine.tui': { date: true, model: false } })
    );
    invalidateStatusSegments();
    const out = await paint('tui');
    const line = (label: string) =>
      out.split('\n').find((l) => l.includes(label)) ?? '';
    expect(line('Date')).toContain('on');
    expect(line('Model')).toContain('off');
  });

  it('reflects a write rather than the value it started with', async () => {
    // Writes are async. A panel holding its own copy and re-reading straight
    // after one would keep showing the previous state.
    const { setStatusSegmentVisible } =
      await import('../../layout/status-line/config.js');
    const terminal = new MockTerminal();
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'v2' });
    active = render(
      React.createElement(
        AppStoreContext.Provider,
        { value: store },
        React.createElement(StatusLineSettingsPanel, {
          surface: 'tui',
          onClose: () => {},
        })
      ),
      { terminal, exitOnCtrlC: false }
    );
    await new Promise((r) => setTimeout(r, 80));
    const dateLine = (out: string) =>
      out.split('\n').find((l) => l.includes('Date')) ?? '';
    expect(dateLine(stripAnsi(terminal.output))).toContain('off');

    terminal.output = '';
    await setStatusSegmentVisible('tui', 'date', true);
    await new Promise((r) => setTimeout(r, 120));
    expect(dateLine(stripAnsi(terminal.output))).toContain('on');
  });

  it('marks the selected row without shifting the columns', async () => {
    const { CURSOR_MARKER } = await import('../../../renderer.js');
    const { rowLead } = await import('../StatusLineSettingsPanel.js');

    const selected = rowLead(true, '>');
    // The marker is the only addition, so the visible width is unchanged and a
    // screen reader can still follow the highlight.
    expect(selected).toContain(CURSOR_MARKER);
    expect(selected.replace(CURSOR_MARKER, '').length).toBe(
      rowLead(false, '>').length
    );
  });

  it('fits a short pane by windowing the list', async () => {
    // 14 segments plus headings do not fit a small pane, and a panel taller than
    // the terminal is simply cut off with no way to reach the rest.
    termSize.height = 20;
    const out = await paint('tui');
    const lines = out.split('\n');

    expect(lines.length).toBeLessThanOrEqual(20);
    expect(out).toContain('more');
    // The far end of the list is out of view rather than silently dropped.
    expect(out).not.toContain('Reset to defaults');
  });

  it('persists only what differs from the default', async () => {
    const { setStatusSegmentVisible } =
      await import('../../layout/status-line/config.js');
    await setStatusSegmentVisible('tui', 'date', true);
    expect(saved('tui')).toEqual({ date: true });

    // Back to the default: the key goes away rather than being written out.
    await setStatusSegmentVisible('tui', 'date', false);
    expect(saved('tui')).toEqual({});
  });
});
