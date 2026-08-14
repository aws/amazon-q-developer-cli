export interface Glyphs {
  dotFilled: string;
  dotEmpty: string;
  dotLoading: string;
  checkmark: string;
  cross: string;
  warning: string;
  chevron: string;
  arrowRight: string;
  arrowDown: string;
  diamond: string;
  eye: string;
  smallDot: string;
  clipboard: string;
  sparkle: string;
  progress0: string;
  progress25: string;
  progress50: string;
  progress75: string;
  executing: string;
  lineHorizontal: string;
  lineHorizontalHeavy: string;
  lineVertical: string;
  cornerTopRight: string;
  cornerBottomRight: string;
  cornerBottomLeft: string;
  cornerBottomLeftRound: string;
  teeLeft: string;
  teeRight: string;
  arrow: string;
  arrowLeft: string;
  cornerTopLeft: string;
  teeTop: string;
  teeBottom: string;
  tableCross: string;
  treeCorner: string;
  treeBranch: string;
  // Extended vocabulary so previously-hardcoded characters can degrade in
  // ASCII mode. Keep UNICODE_GLYPHS and ASCII_GLYPHS key-identical.
  ellipsis: string;
  midEllipsis: string;
  arrowUp: string;
  enter: string;
  triangleLeft: string;
  triangleRight: string;
  loop: string;
  refresh: string;
  codeIntelligence: string;
  times: string;
  pause: string;
  bar: string;
  pencil: string;
  wrench: string;
  emDash: string;
  cloud: string;
  dotDouble: string;
  dotDashed: string;
  search: string;
  rewind: string;
  shift: string;
  gear: string;
}

export interface Spinners {
  quarterSpinner: string[];
  brailleFill: string[];
  pie: string[];
  brailleRotate: string[];
}

export const UNICODE_GLYPHS: Glyphs = {
  dotFilled: '●',
  dotEmpty: '○',
  dotLoading: '◌',
  checkmark: '✓',
  cross: '✗',
  warning: '⚠',
  chevron: '❯',
  arrowRight: '▸',
  arrowDown: '↓',
  diamond: '◇',
  eye: '◉',
  smallDot: '·',
  clipboard: '📋',
  sparkle: '✨',
  progress0: '◷',
  progress25: '◔',
  progress50: '◑',
  progress75: '◕',
  executing: '◐',
  lineHorizontal: '─',
  lineHorizontalHeavy: '━',
  lineVertical: '│',
  cornerTopRight: '┐',
  cornerBottomRight: '┘',
  cornerBottomLeft: '└',
  cornerBottomLeftRound: '╰',
  teeLeft: '┤',
  teeRight: '├',
  arrow: '→',
  arrowLeft: '←',
  cornerTopLeft: '┌',
  teeTop: '┬',
  teeBottom: '┴',
  tableCross: '┼',
  treeCorner: '└──',
  treeBranch: '├──',
  ellipsis: '…',
  midEllipsis: '⋯',
  arrowUp: '↑',
  enter: '↵',
  triangleLeft: '◀',
  triangleRight: '▶',
  loop: '↻',
  refresh: '⟳',
  codeIntelligence: 'λ',
  times: '×',
  pause: '⏸',
  bar: '█',
  pencil: '✎',
  wrench: '🔧',
  emDash: '—',
  cloud: '☁️',
  dotDouble: '◉',
  dotDashed: '◌',
  search: '⌕',
  rewind: '↩',
  shift: '⇧',
  gear: '⚙',
};

export const ASCII_GLYPHS: Glyphs = {
  dotFilled: '*',
  dotEmpty: 'o',
  dotLoading: 'o',
  checkmark: '+',
  cross: 'x',
  warning: '!',
  chevron: '>',
  arrowRight: '>',
  arrowDown: 'v',
  diamond: 'o',
  eye: 'O',
  smallDot: '.',
  clipboard: 's',
  sparkle: '*',
  progress0: '-',
  progress25: '\\',
  progress50: '|',
  progress75: '/',
  executing: '*',
  lineHorizontal: '-',
  lineHorizontalHeavy: '-',
  lineVertical: '|',
  cornerTopRight: '+',
  cornerBottomRight: '+',
  cornerBottomLeft: '+',
  cornerBottomLeftRound: '+',
  teeLeft: '+',
  teeRight: '+',
  arrow: '->',
  arrowLeft: '<',
  cornerTopLeft: '+',
  teeTop: '+',
  teeBottom: '+',
  tableCross: '+',
  treeCorner: '+--',
  treeBranch: '+--',
  ellipsis: '...',
  midEllipsis: '...',
  arrowUp: '^',
  enter: 'enter',
  triangleLeft: '<',
  triangleRight: '>',
  loop: '@',
  refresh: '@',
  codeIntelligence: 'L',
  times: 'x',
  pause: '||',
  bar: '#',
  pencil: 'e',
  wrench: 'T',
  emDash: '-',
  cloud: '*',
  dotDouble: '@',
  dotDashed: 'o',
  search: '?',
  rewind: '<',
  shift: 'shift+',
  gear: '*',
};

export const UNICODE_SPINNERS: Spinners = {
  quarterSpinner: ['◐', '◓', '◑', '◒'],
  brailleFill: ['⠀', '⠁', '⠉', '⠙', '⠹', '⢹', '⣹', '⣽', '⣿'],
  pie: ['◔', '◑', '◕', '●'],
  brailleRotate: ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'],
};

export const ASCII_SPINNERS: Spinners = {
  quarterSpinner: ['-', '\\', '|', '/'],
  brailleFill: ['.', '.', ':', ':', '|', '|', '#', '#', '#'],
  pie: ['-', '\\', '|', '/'],
  brailleRotate: ['-', '\\', '|', '/'],
};

/**
 * Partial-block ramps for meters/progress bars (8 parallel levels). Lets bar
 * fills (ContextBreakdown, UsagePanel, voice meter) degrade in ASCII mode
 * instead of hardcoding the Unicode block ramp.
 */
export const UNICODE_BAR_RAMP: readonly string[] = [
  '▁',
  '▂',
  '▃',
  '▄',
  '▅',
  '▆',
  '▇',
  '█',
];
export const ASCII_BAR_RAMP: readonly string[] = [
  ' ',
  '.',
  ':',
  '-',
  '=',
  '+',
  '*',
  '#',
];

/**
 * Pick the bar ramp matching the current ASCII-art setting. `allowAsciiArt`
 * follows the same convention as useGlyphs (true -> rich Unicode, false ->
 * ASCII fallback).
 */
export const getBarRamp = (allowAsciiArt: boolean): readonly string[] =>
  allowAsciiArt ? UNICODE_BAR_RAMP : ASCII_BAR_RAMP;
