import type { Key } from '../../../hooks/useKeypress.js';

export type InputKeyAction =
  | 'cancel'
  | 'submit'
  | 'delete'
  | 'ignore'
  | { append: string };

function sanitizePaste(input: string): string {
  const firstLine = input.split(/\r?\n/)[0] ?? '';
  // eslint-disable-next-line no-control-regex
  return firstLine.replace(/[\x00-\x1f\x7f]/g, '');
}

export function classifyInputKey(input: string, key: Key): InputKeyAction {
  if (key.escape) return 'cancel';
  if (key.return) return 'submit';
  if (key.backspace || key.delete) return 'delete';
  if (key.paste || input.length > 1) {
    const text = sanitizePaste(input);
    return text ? { append: text } : 'ignore';
  }
  if (input.length === 1 && input >= ' ' && !key.ctrl && !key.meta) {
    return { append: input };
  }
  return 'ignore';
}
