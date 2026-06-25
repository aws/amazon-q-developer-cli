import { describe, test, expect } from 'vitest';
import {
  selectBootIndicatorPhase,
  formatBootIndicator,
  type BootProgressEntry,
  type McpInitEntry,
} from '../boot-indicator.js';

function bootEntry(
  status: 'loading' | 'ready' | 'failed',
  startTime: number,
  label = 'placeholder'
): BootProgressEntry {
  return { label, status, startTime };
}

function mcpEntry(
  status: 'loading' | 'ready' | 'failed',
  startTime: number
): McpInitEntry {
  return { status, startTime };
}

describe('selectBootIndicatorPhase', () => {
  test('returns null when nothing is loading', () => {
    const phase = selectBootIndicatorPhase(
      new Map([['agent_connect', bootEntry('ready', 1000)]]),
      new Map([['m1', mcpEntry('ready', 1000)]]),
      5000
    );
    expect(phase).toBeNull();
  });

  test('returns null on empty maps', () => {
    const phase = selectBootIndicatorPhase(new Map(), new Map(), 5000);
    expect(phase).toBeNull();
  });

  test('agent_connect takes priority over session_create AND MCPs', () => {
    // Even when session_create and MCPs are also "loading", agent_connect
    // is the most fundamental blocker — without ACP handshake nothing
    // else can happen — so it owns the indicator row alone.
    const phase = selectBootIndicatorPhase(
      new Map([
        ['agent_connect', bootEntry('loading', 1000)],
        ['session_create', bootEntry('loading', 1500)],
      ]),
      new Map([
        ['m1', mcpEntry('loading', 1200)],
        ['m2', mcpEntry('loading', 1300)],
      ]),
      4000
    );
    expect(phase).toEqual({
      label: 'Connecting to agent',
      elapsed: 3000,
    });
  });

  test('session_create takes priority over MCPs when agent_connect is done', () => {
    const phase = selectBootIndicatorPhase(
      new Map([
        ['agent_connect', bootEntry('ready', 1000)],
        ['session_create', bootEntry('loading', 2000)],
      ]),
      new Map([['m1', mcpEntry('loading', 2500)]]),
      5000
    );
    expect(phase).toEqual({
      label: 'Initializing workspace',
      elapsed: 3000,
    });
  });

  test('falls through to MCP aggregate when boot stages are settled', () => {
    const phase = selectBootIndicatorPhase(
      new Map([
        ['agent_connect', bootEntry('ready', 1000)],
        ['session_create', bootEntry('ready', 2000)],
      ]),
      new Map([
        ['m1', mcpEntry('ready', 2500)],
        ['m2', mcpEntry('loading', 3000)],
        ['m3', mcpEntry('loading', 3500)],
        ['m4', mcpEntry('failed', 2700)],
      ]),
      8000
    );
    // Settled = 2 (m1 ready + m4 failed), total = 4, label uses both.
    expect(phase).toEqual({
      label: 'Loading 2/4 MCP server(s)',
      elapsed: 5000, // now (8000) - earliestStart (3000)
    });
  });

  test('MCP elapsed measures from the earliest STILL-LOADING server', () => {
    // The whole point of "earliest" is to give a stable ticking timer
    // that doesn't jump around as servers settle. Lock that contract:
    // a settled m1 with an earlier startTime must NOT pull elapsed back.
    const phase = selectBootIndicatorPhase(
      new Map(),
      new Map([
        // m1 finished before m2 started; only m2 is still loading
        ['m1', mcpEntry('ready', 1000)],
        ['m2', mcpEntry('loading', 5000)],
      ]),
      8000
    );
    expect(phase).toEqual({
      label: 'Loading 1/2 MCP server(s)',
      elapsed: 3000, // now (8000) - earliest *loading* startTime (5000)
    });
  });

  test('hides indicator when no entry is in loading state, even with stale records', () => {
    const phase = selectBootIndicatorPhase(
      new Map([['agent_connect', bootEntry('ready', 1000)]]),
      new Map([
        ['m1', mcpEntry('ready', 2000)],
        ['m2', mcpEntry('failed', 2500)],
      ]),
      5000
    );
    expect(phase).toBeNull();
  });

  test('elapsed reflects time since startTime even when phase just started', () => {
    const phase = selectBootIndicatorPhase(
      new Map([['agent_connect', bootEntry('loading', 9990)]]),
      new Map(),
      10000
    );
    expect(phase).toEqual({
      label: 'Connecting to agent',
      elapsed: 10,
    });
  });
});

describe('formatBootIndicator', () => {
  // chalk.dim wraps output in escape codes — strip them to assert visible text.
  // eslint-disable-next-line no-control-regex
  const ansiStrip = (s: string) => s.replace(/\x1B\[[0-9;]*m/g, '');

  test('returns empty string when phase is null', () => {
    expect(formatBootIndicator(null, '⠋')).toBe('');
  });

  // Sub-second phases suppress the elapsed counter so a fast loader doesn't
  // flash "(0.0s)"; >1s shows it rounded to one decimal place.
  test.each([
    [
      '<= 1s suppresses elapsed',
      'Connecting to agent',
      250,
      '⠋',
      '  ⠋ Connecting to agent…',
    ],
    [
      '> 1s shows elapsed',
      'Loading 3/12 MCP server(s)',
      13700,
      '⠙',
      '  ⠙ Loading 3/12 MCP server(s)… (13.7s)',
    ],
    [
      'rounds to 1 decimal',
      'Initializing workspace',
      10723,
      '⠹',
      '  ⠹ Initializing workspace… (10.7s)',
    ],
  ])('%s', (_name, label, elapsed, glyph, expected) => {
    expect(ansiStrip(formatBootIndicator({ label, elapsed }, glyph))).toBe(
      expected
    );
  });

  test('threshold is exclusive — exactly 1s suppresses elapsed', () => {
    // Boundary check: the user observed "(10.7s)" in the original bug
    // report, so the >1000 cutoff matters for accuracy. 1000 itself
    // stays suppressed; just-over (1001) shows.
    const at = formatBootIndicator({ label: 'X', elapsed: 1000 }, '⠋');
    const just = formatBootIndicator({ label: 'X', elapsed: 1001 }, '⠋');
    expect(ansiStrip(at)).toBe('  ⠋ X…');
    expect(ansiStrip(just)).toBe('  ⠋ X… (1.0s)');
  });
});
