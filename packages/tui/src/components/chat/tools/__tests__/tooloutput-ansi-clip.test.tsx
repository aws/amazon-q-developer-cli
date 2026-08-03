import { afterEach, beforeEach, describe, expect, test } from 'vitest';
import React from 'react';
import stripAnsi from 'strip-ansi';
import { ToolOutput } from '../ToolOutput.js';
import { ToolUseMessage } from '../../../ui/ToolUseMessage.js';
import { ThemeProvider } from '../../../../theme/ThemeProvider.js';
import { GlyphsProvider } from '../../../../hooks/useGlyphs.js';
import { chalk } from '../../../../utils/color.js';
import { renderRaw, renderRawWithProviders } from './twinki-render.js';
import {
  resetVerboseCache,
  setVerboseConfig,
} from '../../../../lite/verbose.js';
import { useTempKiroHome as prepareTempKiroHome } from '../../../../lite/__tests__/temp-kiro-home.js';
import { ToolUseStatus } from '../../../../stores/app-store.js';
import {
  boundToolOutputLine,
  MAX_TOOL_OUTPUT_LINE_CHARS,
  wrapAnsiLine,
} from '../../../../lite/render.js';

// Force color so the ANSI-clip path is exercised.
chalk.level = 3;
prepareTempKiroHome();
// Pin the rollout cohort per-test: the in-cohort ToolOutput body path only
// renders when the flag is set, and a sibling suite in the same `bun test`
// batch can leave it cleared (afterEach delete), so depend on ambient env at
// your peril — set it here (restored after) to keep this suite hermetic.
let priorRollout: string | undefined;
beforeEach(() => {
  // Re-pin per-test: a sibling suite (RepoPickerPanel) save/restores chalk.level
  // around its own block and can leave it below 3, which would suppress the SGR
  // codes this ANSI suite asserts on. Module-load assignment isn't enough.
  chalk.level = 3;
  priorRollout = process.env.KIRO_LITE_ROLLOUT_ENABLED;
  process.env.KIRO_LITE_ROLLOUT_ENABLED = '1';
  resetVerboseCache();
});
afterEach(() => {
  if (priorRollout === undefined) delete process.env.KIRO_LITE_ROLLOUT_ENABLED;
  else process.env.KIRO_LITE_ROLLOUT_ENABLED = priorRollout;
});

const wrap = (node: React.ReactElement) => (
  <ThemeProvider>
    <GlyphsProvider>{node}</GlyphsProvider>
  </ThemeProvider>
);

describe('ToolOutput ANSI-aware per-line clip (outputMaxChars)', () => {
  // Regression: a self-colored line clipped by outputMaxChars must clip by
  // VISIBLE width and keep a trailing reset, else the color bleeds into the
  // rows below (or a mid-escape cut garbles bytes). Plain lines keep the tint.
  test('a colored line clips by visible width and stays reset-terminated', async () => {
    setVerboseConfig(
      { display: { outputMaxLines: null, outputMaxChars: 8 } },
      'tui'
    );
    const colored = chalk.red('R'.repeat(40));
    const raw = await renderRaw(wrap(<ToolOutput lines={[colored]} />));
    // Visible content is clipped to the cap (7 visible + ellipsis), not 40.
    const visible = stripAnsi(raw);
    expect(visible).toContain('RRRRRRR…'); // 7 R's + ellipsis (max-1 rule)
    expect(visible).not.toContain('R'.repeat(40));
    // The clipped colored line re-appends a reset so color can't bleed down.
    expect(raw).toContain('\x1b[0m');
  });

  test('a plain line clips naively and keeps the sage-green tint', async () => {
    setVerboseConfig(
      { display: { outputMaxLines: null, outputMaxChars: 8 } },
      'tui'
    );
    const raw = await renderRaw(
      wrap(<ToolOutput lines={['plain-' + 'x'.repeat(40)]} />)
    );
    const visible = stripAnsi(raw);
    expect(visible).toContain('plain-x…'); // 7 chars + ellipsis (max-1 rule)
    // The plain body is tinted (bodyColor), not rendered raw: an SGR sequence
    // (ESC[…m) immediately precedes the clipped text.
    const ESC = String.fromCharCode(27);
    expect(raw).toContain(`${ESC}[`);
    expect(raw.includes(`${ESC}[`) && raw.includes('plain-x…')).toBe(true);
  });

  test('wrapAnsiLine carries color onto continuation rows', () => {
    const RED = `${String.fromCharCode(27)}[31m`;
    const rows = wrapAnsiLine(chalk.red(`HEAD_${'R'.repeat(80)}_TAIL`), 16, 16);
    expect(rows.length).toBeGreaterThan(1);
    expect(stripAnsi(rows.at(-1) ?? '')).toContain('TAIL');
    expect(rows.at(-1)?.startsWith(RED)).toBe(true);
  });

  test('a streaming tail keeps its marker, tail text, and carried color', async () => {
    setVerboseConfig(
      {
        filters: ['all'],
        display: { outputMaxLines: 1, outputMaxChars: null },
      },
      'tui'
    );
    const RED = `${String.fromCharCode(27)}[31m`;
    const colored = chalk.red(`HEAD_${'R'.repeat(160)}_TAIL`);
    const raw = await renderRawWithProviders(
      <ToolUseMessage
        id="colored-stream"
        name="execute_bash"
        content={JSON.stringify({ command: 'colored-stream' })}
        isFinished={false}
        status={ToolUseStatus.Approved}
      />,
      {
        columns: 40,
        configureStore: (store) =>
          store.setState({
            liveOutputs: new Map([['colored-stream', [[colored]]]]),
          }),
      }
    );
    const visible = stripAnsi(raw);
    expect(visible).toContain('...+');
    expect(visible).toContain('lines above');
    expect(visible).toContain('TAIL');
    expect(visible).not.toContain('HEAD');
    const tail = raw.lastIndexOf('TAIL');
    const lineStart = raw.lastIndexOf('\n', tail);
    expect(raw.lastIndexOf(RED, tail)).toBeGreaterThan(lineStart);
  });

  test('a pathological source line is bounded before visual wrapping', async () => {
    setVerboseConfig(
      { display: { outputMaxLines: 5, outputMaxChars: null } },
      'tui'
    );
    const raw = await renderRaw(
      wrap(
        <ToolOutput
          lines={[`HEAD_${'x'.repeat(1_000_000)}_TAIL`]}
          previewPosition="start"
        />
      ),
      { columns: 80, rows: 40 }
    );
    const visible = stripAnsi(raw);
    expect(visible).toContain('HEAD_');
    expect(visible).toContain('...+');
    expect(visible).toContain('(ctrl+o to toggle)');
    expect(visible).not.toContain('_TAIL');
  });
});

describe('boundToolOutputLine', () => {
  test('does not split surrogate pairs at either retained edge', () => {
    const head = boundToolOutputLine(
      `${'x'.repeat(MAX_TOOL_OUTPUT_LINE_CHARS - 1)}😀tail`,
      'start'
    );
    const tail = boundToolOutputLine(
      `head😀${'x'.repeat(MAX_TOOL_OUTPUT_LINE_CHARS - 1)}`,
      'end'
    );

    expect(head.text).toBe('x'.repeat(MAX_TOOL_OUTPUT_LINE_CHARS - 1));
    expect(head.text).not.toContain('\ud83d');
    expect(tail.text).toBe('x'.repeat(MAX_TOOL_OUTPUT_LINE_CHARS - 1));
    expect(tail.text).not.toContain('\ude00');
  });

  test('keeps SGR sequences valid and restores color on a retained tail', () => {
    const RED = '\x1b[31m';
    const RESET = '\x1b[0m';
    const head = boundToolOutputLine(
      `${RED}${'x'.repeat(MAX_TOOL_OUTPUT_LINE_CHARS)}tail`,
      'start'
    );
    const tail = boundToolOutputLine(
      `${RED}head${'x'.repeat(MAX_TOOL_OUTPUT_LINE_CHARS)}${RESET}`,
      'end'
    );

    expect(head.text.endsWith(RESET)).toBe(true);
    expect(tail.text.startsWith(RED)).toBe(true);
    expect(tail.text.endsWith(RESET)).toBe(true);
    expect(stripAnsi(tail.text)).not.toContain('head');
  });

  test('compacts repeated SGR state instead of rebuilding an unbounded prefix', () => {
    const RED = '\x1b[31m';
    const source = RED.repeat(100_000) + 'x'.repeat(MAX_TOOL_OUTPUT_LINE_CHARS);
    const bounded = boundToolOutputLine(source, 'end');

    expect(bounded.text.startsWith(RED)).toBe(true);
    expect(bounded.text.length).toBeLessThanOrEqual(
      MAX_TOOL_OUTPUT_LINE_CHARS + RED.length
    );
  });

  test('backs up rather than retaining a partial SGR escape', () => {
    const prefix = 'x'.repeat(MAX_TOOL_OUTPUT_LINE_CHARS - 2);
    const bounded = boundToolOutputLine(`${prefix}\x1b[31mTAIL`, 'start');

    expect(bounded.text).toBe(prefix);
    expect(bounded.droppedChars).toBe(9);
  });
});
