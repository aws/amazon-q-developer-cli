import type { Terminal } from '@xterm/headless';

export type TerminalColor =
  | { mode: 'default' }
  | { mode: 'palette'; value: number }
  | { mode: 'rgb'; value: number };

export interface TerminalCellStyle {
  foreground: TerminalColor;
  background: TerminalColor;
  bold: boolean;
  dim: boolean;
  italic: boolean;
  underline: boolean;
  blink: boolean;
  inverse: boolean;
  invisible: boolean;
  strikethrough: boolean;
  overline: boolean;
}

export interface TerminalCell {
  text: string;
  width: 0 | 1 | 2;
  styleIndex: number;
}

export interface TerminalFrame {
  schemaVersion: 1;
  viewport: {
    columns: number;
    rows: number;
  };
  buffer: 'normal' | 'alternate';
  cursor: {
    column: number;
    row: number;
  };
  styles: readonly TerminalCellStyle[];
  rows: readonly (readonly TerminalCell[])[];
}

export interface TerminalFrameDiff {
  equal: boolean;
  viewportChanged: boolean;
  bufferChanged: boolean;
  cursorChanged: boolean;
  changedCells: number;
}

export type SerializedTerminalCell =
  | readonly [text: string, width: 0 | 1 | 2]
  | readonly [text: string, width: 0 | 1 | 2, repeat: number];

export type SerializedTerminalRun = readonly [
  styleIndex: number,
  cells: readonly SerializedTerminalCell[],
];

export interface SerializedTerminalFrame {
  schemaVersion: 1;
  viewport: readonly [columns: number, rows: number];
  buffer: 'normal' | 'alternate';
  cursor: readonly [column: number, row: number];
  styles: readonly TerminalCellStyle[];
  rows: readonly (readonly SerializedTerminalRun[])[];
}

const DEFAULT_STYLE: TerminalCellStyle = {
  foreground: { mode: 'default' },
  background: { mode: 'default' },
  bold: false,
  dim: false,
  italic: false,
  underline: false,
  blink: false,
  inverse: false,
  invisible: false,
  strikethrough: false,
  overline: false,
};

const DEFAULT_FOREGROUND = '#c9d1d9';
const DEFAULT_BACKGROUND = '#0d1117';
const BASIC_PALETTE = [
  '#000000',
  '#cd0000',
  '#00cd00',
  '#cdcd00',
  '#0000ee',
  '#cd00cd',
  '#00cdcd',
  '#e5e5e5',
  '#7f7f7f',
  '#ff0000',
  '#00ff00',
  '#ffff00',
  '#5c5cff',
  '#ff00ff',
  '#00ffff',
  '#ffffff',
] as const;

function color(
  mode: 'foreground' | 'background',
  cell: NonNullable<
    ReturnType<
      NonNullable<
        ReturnType<Terminal['buffer']['active']['getLine']>
      >['getCell']
    >
  >
): TerminalColor {
  const isRgb = mode === 'foreground' ? cell.isFgRGB() : cell.isBgRGB();
  const isPalette =
    mode === 'foreground' ? cell.isFgPalette() : cell.isBgPalette();
  const value = mode === 'foreground' ? cell.getFgColor() : cell.getBgColor();
  if (isRgb) return { mode: 'rgb', value };
  if (isPalette) return { mode: 'palette', value };
  return { mode: 'default' };
}

function cellStyle(
  cell: NonNullable<
    ReturnType<
      NonNullable<
        ReturnType<Terminal['buffer']['active']['getLine']>
      >['getCell']
    >
  >
): TerminalCellStyle {
  return {
    foreground: color('foreground', cell),
    background: color('background', cell),
    bold: cell.isBold() !== 0,
    dim: cell.isDim() !== 0,
    italic: cell.isItalic() !== 0,
    underline: cell.isUnderline() !== 0,
    blink: cell.isBlink() !== 0,
    inverse: cell.isInverse() !== 0,
    invisible: cell.isInvisible() !== 0,
    strikethrough: cell.isStrikethrough() !== 0,
    overline: cell.isOverline() !== 0,
  };
}

function styleKey(style: TerminalCellStyle): string {
  return JSON.stringify(style);
}

function width(value: number): 0 | 1 | 2 {
  if (value === 0 || value === 2) return value;
  return 1;
}

export function readVisibleTerminalFrame(terminal: Terminal): TerminalFrame {
  const buffer = terminal.buffer.active;
  const stylesByKey = new Map<string, TerminalCellStyle>();
  const rowStyles: string[][] = [];
  const rows: TerminalCell[][] = [];

  for (
    let bufferRow = buffer.baseY;
    bufferRow < buffer.baseY + terminal.rows;
    bufferRow += 1
  ) {
    const line = buffer.getLine(bufferRow);
    const cells: TerminalCell[] = [];
    const styleKeys: string[] = [];
    for (let column = 0; column < terminal.cols; column += 1) {
      const current = line?.getCell(column);
      if (!current) {
        const fallback: TerminalCellStyle = {
          foreground: { mode: 'default' },
          background: { mode: 'default' },
          bold: false,
          dim: false,
          italic: false,
          underline: false,
          blink: false,
          inverse: false,
          invisible: false,
          strikethrough: false,
          overline: false,
        };
        const key = styleKey(fallback);
        stylesByKey.set(key, fallback);
        styleKeys.push(key);
        cells.push({ text: ' ', width: 1, styleIndex: -1 });
        continue;
      }
      const style = cellStyle(current);
      const key = styleKey(style);
      stylesByKey.set(key, style);
      styleKeys.push(key);
      const cellWidth = width(current.getWidth());
      cells.push({
        text: current.getChars() || (cellWidth === 0 ? '' : ' '),
        width: cellWidth,
        styleIndex: -1,
      });
    }
    rows.push(cells);
    rowStyles.push(styleKeys);
  }

  const styleEntries = [...stylesByKey.entries()].sort(([left], [right]) =>
    left.localeCompare(right)
  );
  const styleIndices = new Map(
    styleEntries.map(([key], index) => [key, index])
  );
  rows.forEach((cells, row) => {
    cells.forEach((cell, column) => {
      cell.styleIndex = styleIndices.get(rowStyles[row]![column]!)!;
    });
  });

  return {
    schemaVersion: 1,
    viewport: { columns: terminal.cols, rows: terminal.rows },
    buffer: buffer === terminal.buffer.alternate ? 'alternate' : 'normal',
    cursor: { column: buffer.cursorX, row: buffer.cursorY },
    styles: styleEntries.map(([, style]) => style),
    rows,
  };
}

export function terminalFrameText(frame: TerminalFrame): string[] {
  return frame.rows.map((row) =>
    row
      .filter((cell) => cell.width !== 0)
      .map((cell) => cell.text)
      .join('')
  );
}

export function serializeTerminalFrame(
  frame: TerminalFrame
): SerializedTerminalFrame {
  return {
    schemaVersion: 1,
    viewport: [frame.viewport.columns, frame.viewport.rows],
    buffer: frame.buffer,
    cursor: [frame.cursor.column, frame.cursor.row],
    styles: frame.styles,
    rows: frame.rows.map((row) => {
      const runs: Array<[number, SerializedTerminalCell[]]> = [];
      for (const cell of row) {
        let run = runs[runs.length - 1];
        if (!run || run[0] !== cell.styleIndex) {
          run = [cell.styleIndex, []];
          runs.push(run);
        }
        const cells = run[1];
        const previous = cells[cells.length - 1];
        if (
          previous &&
          previous[0] === cell.text &&
          previous[1] === cell.width
        ) {
          cells[cells.length - 1] = [
            cell.text,
            cell.width,
            (previous[2] ?? 1) + 1,
          ];
        } else {
          cells.push([cell.text, cell.width]);
        }
      }
      return runs;
    }),
  };
}

export function serializeDiagnosticTextFrame(
  viewport: { columns: number; rows: number },
  lines: readonly string[]
): SerializedTerminalFrame {
  const rows = Array.from({ length: viewport.rows }, (_, row) => {
    const characters = Array.from(lines[row] ?? '').slice(0, viewport.columns);
    return Array.from({ length: viewport.columns }, (_, column) => ({
      text: characters[column] ?? ' ',
      width: 1 as const,
      styleIndex: 0,
    }));
  });
  return serializeTerminalFrame({
    schemaVersion: 1,
    viewport,
    buffer: 'normal',
    cursor: { column: 0, row: 0 },
    styles: [DEFAULT_STYLE],
    rows,
  });
}

export function parseTerminalFrame(
  serialized: SerializedTerminalFrame
): TerminalFrame {
  if (serialized.schemaVersion !== 1) {
    throw new Error(
      `Unsupported terminal frame schema ${String(serialized.schemaVersion)}`
    );
  }
  const rows = serialized.rows.map((runs) =>
    runs.flatMap(([styleIndex, cells]) =>
      cells.flatMap(([text, cellWidth, repeat = 1]) =>
        Array.from({ length: repeat }, () => ({
          text,
          width: cellWidth,
          styleIndex,
        }))
      )
    )
  );
  const [columns, rowCount] = serialized.viewport;
  if (rows.length !== rowCount || rows.some((row) => row.length !== columns)) {
    throw new Error('Serialized terminal frame does not match its viewport');
  }
  return {
    schemaVersion: 1,
    viewport: { columns, rows: rowCount },
    buffer: serialized.buffer,
    cursor: {
      column: serialized.cursor[0],
      row: serialized.cursor[1],
    },
    styles: serialized.styles,
    rows,
  };
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function paletteColor(index: number): string {
  if (index < BASIC_PALETTE.length) return BASIC_PALETTE[index]!;
  if (index < 232) {
    const offset = index - 16;
    const channel = (value: number): number =>
      value === 0 ? 0 : value * 40 + 55;
    const red = channel(Math.floor(offset / 36));
    const green = channel(Math.floor((offset % 36) / 6));
    const blue = channel(offset % 6);
    return `rgb(${red},${green},${blue})`;
  }
  const gray = Math.max(0, Math.min(255, (index - 232) * 10 + 8));
  return `rgb(${gray},${gray},${gray})`;
}

function cssColor(colorValue: TerminalColor, fallback: string): string {
  if (colorValue.mode === 'default') return fallback;
  if (colorValue.mode === 'palette') return paletteColor(colorValue.value);
  return `#${colorValue.value.toString(16).padStart(6, '0')}`;
}

function styleDeclaration(style: TerminalCellStyle): string {
  let foreground = cssColor(style.foreground, DEFAULT_FOREGROUND);
  let background = cssColor(style.background, DEFAULT_BACKGROUND);
  if (style.inverse) [foreground, background] = [background, foreground];
  const declarations = [`color:${foreground}`, `background:${background}`];
  if (style.bold) declarations.push('font-weight:bold');
  if (style.dim) declarations.push('opacity:0.5');
  if (style.italic) declarations.push('font-style:italic');
  const decorations = [
    style.underline ? 'underline' : '',
    style.strikethrough ? 'line-through' : '',
    style.overline ? 'overline' : '',
  ].filter(Boolean);
  if (decorations.length > 0) {
    declarations.push(`text-decoration:${decorations.join(' ')}`);
  }
  if (style.invisible) declarations.push('visibility:hidden');
  return declarations.join(';');
}

export function renderTerminalFrameHtml(frame: TerminalFrame): string {
  const rows = frame.rows.map((row) => {
    let html = '';
    let activeStyle = -1;
    let spanOpen = false;
    for (const cell of row) {
      if (cell.width === 0) continue;
      if (cell.styleIndex !== activeStyle) {
        if (spanOpen) html += '</span>';
        html += `<span style="${styleDeclaration(frame.styles[cell.styleIndex]!)}">`;
        activeStyle = cell.styleIndex;
        spanOpen = true;
      }
      html += escapeHtml(cell.text);
    }
    if (spanOpen) html += '</span>';
    return html;
  });
  return `<pre data-terminal-frame="1" style="font-family:monospace;background:${DEFAULT_BACKGROUND};color:${DEFAULT_FOREGROUND};padding:10px;margin:0">${rows.join('\n')}</pre>`;
}

export function compareTerminalFrames(
  expected: TerminalFrame,
  actual: TerminalFrame
): TerminalFrameDiff {
  const viewportChanged =
    expected.viewport.columns !== actual.viewport.columns ||
    expected.viewport.rows !== actual.viewport.rows;
  const bufferChanged = expected.buffer !== actual.buffer;
  const cursorChanged =
    expected.cursor.column !== actual.cursor.column ||
    expected.cursor.row !== actual.cursor.row;
  let changedCells = 0;
  const rows = Math.max(expected.rows.length, actual.rows.length);
  for (let row = 0; row < rows; row += 1) {
    const expectedRow = expected.rows[row] ?? [];
    const actualRow = actual.rows[row] ?? [];
    const columns = Math.max(expectedRow.length, actualRow.length);
    for (let column = 0; column < columns; column += 1) {
      const expectedCell = expectedRow[column];
      const actualCell = actualRow[column];
      if (!expectedCell || !actualCell) {
        changedCells += 1;
        continue;
      }
      const expectedStyle = expected.styles[expectedCell.styleIndex];
      const actualStyle = actual.styles[actualCell.styleIndex];
      if (
        expectedCell.text !== actualCell.text ||
        expectedCell.width !== actualCell.width ||
        styleKey(expectedStyle!) !== styleKey(actualStyle!)
      ) {
        changedCells += 1;
      }
    }
  }
  return {
    equal:
      !viewportChanged &&
      !bufferChanged &&
      !cursorChanged &&
      changedCells === 0,
    viewportChanged,
    bufferChanged,
    cursorChanged,
    changedCells,
  };
}
