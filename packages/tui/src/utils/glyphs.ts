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
  lineVertical: string;
  cornerTopRight: string;
  cornerBottomRight: string;
  cornerBottomLeft: string;
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
  lineVertical: '│',
  cornerTopRight: '┐',
  cornerBottomRight: '┘',
  cornerBottomLeft: '└',
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
  lineVertical: '|',
  cornerTopRight: '+',
  cornerBottomRight: '+',
  cornerBottomLeft: '+',
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
