// Detect terminal color capabilities
export function getTerminalColorSupport():
  | 'truecolor'
  | '256color'
  | '16color'
  | 'basic' {
  // Check for truecolor support
  if (
    process.env.COLORTERM === 'truecolor' ||
    process.env.COLORTERM === '24bit'
  ) {
    return 'truecolor';
  }

  // Check TERM environment variable
  const term = process.env.TERM || '';

  if (term.includes('truecolor') || term.includes('24bit')) {
    return 'truecolor';
  }

  if (
    term.includes('256') ||
    term === 'xterm-256color' ||
    term === 'screen-256color'
  ) {
    return '256color';
  }

  if (term.includes('color') || term === 'xterm' || term === 'screen') {
    return '16color';
  }

  return 'basic';
}

export function supportsTrueColor(): boolean {
  return getTerminalColorSupport() === 'truecolor';
}

export function supports256Color(): boolean {
  const support = getTerminalColorSupport();
  return support === 'truecolor' || support === '256color';
}
