/**
 * Render tests for status-line segment visibility across both surfaces.
 *
 * The failure this guards is a segment reaching one surface and silently not the
 * other, which is why every case asserts on the full TUI and lite together. The
 * default case additionally pins that turning nothing on leaves the bar as it was.
 */
import {
  describe,
  it,
  expect,
  afterEach,
  beforeEach,
  afterAll,
} from 'bun:test';
import React from 'react';
import { render, type Instance, type Terminal } from 'twinki';
import { mkdtempSync, rmSync, mkdirSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';
import stripAnsi from 'strip-ansi';

const tempHome = mkdtempSync(join(tmpdir(), 'kiro-status-render-test-'));
const originalHome = process.env.KIRO_HOME;
// Set per test rather than here: a module-scope assignment outlives this file
// and would point other suites at these fixtures.
mkdirSync(join(tempHome, 'settings'), { recursive: true });

const { TuiStatusSurface } = await import('../../tui-status-surface.js');
const { LiteStatusSurface } = await import('../../lite/status-surface.js');
const { STATUS_SEGMENT_IDS } = await import('../segments.js');
type StatusSurfaceProps = import('../../status-surface.js').StatusSurfaceProps;

class MockTerminal implements Terminal {
  public output = '';
  constructor(private readonly width = 200) {}
  get columns() {
    return this.width;
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

const props: StatusSurfaceProps = {
  agentName: 'kiro-dev',
  modelName: 'Claude Fable 5',
  effort: 'high',
  contextUsagePercent: 22,
  workspacePath: '/work/kiro-cli',
  gitBranch: 'my-branch',
  goalStatus: null,
  codeIntelligenceActive: true,
  now: new Date(2026, 6, 28, 4, 5),
  usagePercent: 20,
  creditsRemaining: 7960,
};

const { invalidateStatusSegments } = await import('../config.js');
let active: Instance | null = null;

function writeConfig(surface: 'tui' | 'lite', override: unknown): void {
  invalidateStatusSegments();
  writeFileSync(
    join(tempHome, 'settings', 'cli.json'),
    JSON.stringify({ [`chat.statusLine.${surface}`]: override })
  );
}

async function paint(
  surface: 'tui' | 'lite',
  extra: Partial<StatusSurfaceProps> = {}
): Promise<string> {
  const Surface = surface === 'tui' ? TuiStatusSurface : LiteStatusSurface;
  const terminal = new MockTerminal();
  active = render(
    React.createElement(Surface as never, { ...props, ...extra }),
    { terminal, exitOnCtrlC: false }
  );
  await new Promise((r) => setTimeout(r, 60));
  active.unmount();
  active = null;
  return stripAnsi(terminal.output);
}

/** Paint both surfaces under the same override. */
async function paintBoth(override: unknown): Promise<{
  tui: string;
  lite: string;
}> {
  writeConfig('tui', override);
  const tui = await paint('tui');
  writeConfig('lite', override);
  const lite = await paint('lite');
  return { tui, lite };
}

beforeEach(() => {
  process.env.KIRO_HOME = tempHome;
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

describe('status-line segment rendering', () => {
  it('paints the pre-existing set and nothing new by default', async () => {
    const { tui, lite } = await paintBoth({});
    for (const out of [tui, lite]) {
      expect(out).toContain('kiro-dev');
      expect(out).toContain('Claude Fable 5');
      expect(out).toContain('High');
      expect(out).toContain('22%');
      expect(out).toContain('my-branch');
      // New segments stay off even though their data is present.
      expect(out).not.toContain('2026-07-28');
      expect(out).not.toContain('04:05');
      expect(out).not.toContain('usage');
      expect(out).not.toContain('credits');
    }
  });

  it('shows the clock on both surfaces when enabled', async () => {
    const { tui, lite } = await paintBoth({ date: true, time: true });
    for (const out of [tui, lite]) {
      expect(out).toContain('2026-07-28');
      expect(out).toContain('04:05');
    }
  });

  it('shows usage and credits on both surfaces when enabled', async () => {
    const { tui, lite } = await paintBoth({ usage: true, credits: true });
    for (const out of [tui, lite]) {
      expect(out).toContain('20%');
      expect(out).toContain('7,960');
      expect(out).toContain('credits');
    }
  });

  it('keeps the new segments legible while the bar is dimmed', async () => {
    // The dimmed bar takes a different colour path, and goal drops out of it
    // entirely, so painting the new segments there is not implied by the lit bar.
    const on = { date: true, time: true, usage: true, credits: true };
    writeConfig('tui', on);
    const tui = await paint('tui', { dimmed: true });
    writeConfig('lite', on);
    const lite = await paint('lite', { dimmed: true });

    for (const out of [tui, lite]) {
      expect(out).toContain('2026-07-28');
      expect(out).toContain('04:05');
      expect(out).toContain('20%');
      expect(out).toContain('7,960');
    }
  });

  it('hides a default segment on both surfaces when turned off', async () => {
    const { tui, lite } = await paintBoth({ effort: false, branch: false });
    for (const out of [tui, lite]) {
      expect(out).not.toContain('High');
      expect(out).not.toContain('my-branch');
      // Neighbours survive.
      expect(out).toContain('Claude Fable 5');
      expect(out).toContain('22%');
    }
  });

  it('keeps location right-aligned on the full TUI regardless of visibility', async () => {
    writeConfig('tui', { time: true, usage: true });
    const out = await paint('tui');
    // Everything on the left group precedes the location/branch pair.
    expect(out.indexOf('04:05')).toBeLessThan(out.indexOf('/work/kiro-cli'));
    expect(out.indexOf('/work/kiro-cli')).toBeLessThan(
      out.indexOf('my-branch')
    );
  });

  it('keeps lite goal trailing the location and branch', async () => {
    writeConfig('lite', {});
    const out = await paint('lite');
    expect(out.indexOf('/work/kiro-cli')).toBeLessThan(
      out.indexOf('my-branch')
    );
  });

  it('paints nothing when every segment is off', async () => {
    const allOff = Object.fromEntries(
      STATUS_SEGMENT_IDS.map((id) => [id, false])
    );
    const { tui, lite } = await paintBoth(allOff);
    for (const out of [tui, lite]) {
      expect(out).not.toContain('kiro-dev');
      expect(out).not.toContain('Claude Fable 5');
      expect(out).not.toContain('/work/kiro-cli');
      // Nothing to separate, so no separator should survive either.
      expect(stripAnsi(out)).not.toContain('·');
      expect(stripAnsi(out).trim()).toBe('');
    }
  });
});
