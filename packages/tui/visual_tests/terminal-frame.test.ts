import { describe, expect, test } from 'bun:test';
import { Terminal } from '@xterm/headless';
import { PtyManager } from '../src/test-utils/shared/pty-manager.js';
import {
  compareTerminalFrames,
  parseTerminalFrame,
  readVisibleTerminalFrame,
  renderTerminalFrameHtml,
  serializeDiagnosticTextFrame,
  serializeTerminalFrame,
  terminalFrameText,
} from '../src/test-utils/shared/terminal-frame.js';

function write(terminal: Terminal, data: string): Promise<void> {
  return new Promise((resolve) => terminal.write(data, resolve));
}

async function frameFor(
  data: string,
  options: { columns?: number; rows?: number } = {}
) {
  const terminal = new Terminal({
    cols: options.columns ?? 8,
    rows: options.rows ?? 3,
    allowProposedApi: true,
  });
  await write(terminal, data);
  const frame = readVisibleTerminalFrame(terminal);
  terminal.dispose();
  return frame;
}

describe('terminal frame', () => {
  test('captures exactly the visible viewport without scrollback', async () => {
    const frame = await frameFor('first\r\nsecond\r\nthird', {
      columns: 8,
      rows: 2,
    });

    expect(frame.rows).toHaveLength(2);
    expect(frame.rows.every((row) => row.length === 8)).toBe(true);
    expect(terminalFrameText(frame)).toEqual(['second  ', 'third   ']);
  });

  test('captures a settled PTY frame and latches its exit code', async () => {
    const pty = new PtyManager({ width: 12, height: 2 });
    try {
      expect(pty.getExitCode()).toBeUndefined();
      pty.spawn(process.execPath, [
        '-e',
        'process.stdout.write("\\u001b[31mREADY\\u001b[0m"); setTimeout(() => process.exit(7), 100)',
      ]);

      await pty.waitForText('READY', 2000);
      const frame = await pty.captureVisibleFrame({
        quietMs: 10,
        timeoutMs: 2000,
      });
      expect(terminalFrameText(frame)[0]).toContain('READY');
      expect(await pty.expectExit(2000)).toBe(7);
      expect(pty.getExitCode()).toBe(7);
    } finally {
      pty.kill();
      pty.dispose();
    }
  });

  test('preserves text, width, color, and style in one frame', async () => {
    const frame = await frameFor('\x1b[38;2;1;2;3m\x1b[1;3;4mA界\x1b[0m');
    const firstRow = frame.rows[0]!;
    const styled = frame.styles[firstRow[0]!.styleIndex]!;

    expect(firstRow[0]).toMatchObject({ text: 'A', width: 1 });
    expect(firstRow[1]).toMatchObject({ text: '界', width: 2 });
    expect(firstRow[2]).toMatchObject({ text: '', width: 0 });
    expect(styled).toMatchObject({
      foreground: { mode: 'rgb', value: 0x010203 },
      bold: true,
      italic: true,
      underline: true,
    });

    const html = renderTerminalFrameHtml(frame);
    expect(html).toContain('data-terminal-frame="1"');
    expect(html).toContain('color:#010203');
    expect(html).toContain('font-weight:bold');
    expect(html).toContain('A界');
    expect(html).not.toContain('\x1b');
  });

  test('compares rendered cells instead of raw ANSI bytes', async () => {
    const shortAnsi = await frameFor('\x1b[31mred\x1b[0m');
    const verboseAnsi = await frameFor(
      '\x1b[0m\x1b[31mred\x1b[39m\x1b[49m\x1b[0m'
    );
    const changedStyle = await frameFor('\x1b[32mred\x1b[0m');

    expect(compareTerminalFrames(shortAnsi, verboseAnsi)).toEqual({
      equal: true,
      viewportChanged: false,
      bufferChanged: false,
      cursorChanged: false,
      changedCells: 0,
    });
    expect(compareTerminalFrames(shortAnsi, changedStyle)).toMatchObject({
      equal: false,
      changedCells: 3,
    });
  });

  test('round-trips a terminal frame through compact storage', async () => {
    const frame = await frameFor(
      '\x1b[31mred\x1b[0m  \x1b[38;2;1;2;3m界\x1b[0m'
    );

    expect(parseTerminalFrame(serializeTerminalFrame(frame))).toEqual(frame);
  });

  test('compresses repeated blank cells within a style run', async () => {
    const frame = await frameFor('x', { columns: 8, rows: 2 });
    const serialized = serializeTerminalFrame(frame);

    expect(
      serialized.rows.some((runs) =>
        runs.some(([, cells]) =>
          cells.some(
            ([text, cellWidth, repeat]) =>
              text === ' ' && cellWidth === 1 && (repeat ?? 1) > 1
          )
        )
      )
    ).toBe(true);
  });

  test('creates valid diagnostic evidence when terminal capture fails', () => {
    const serialized = serializeDiagnosticTextFrame({ columns: 8, rows: 3 }, [
      'failure output',
      'second',
    ]);
    const frame = parseTerminalFrame(serialized);

    expect(frame.viewport).toEqual({ columns: 8, rows: 3 });
    expect(terminalFrameText(frame)).toEqual([
      'failure ',
      'second  ',
      '        ',
    ]);
  });
});
