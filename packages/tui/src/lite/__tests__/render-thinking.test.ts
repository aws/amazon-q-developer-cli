import { test, expect } from 'vitest';
import { renderThinkingBlock } from '../render.js';
import stripAnsi from 'strip-ansi';

function rules(cols?: number): [number, number] {
  const lines = stripAnsi(
    renderThinkingBlock('reasoning', undefined, cols)
  ).split('\n');
  return [lines[0]!.length, lines.at(-1)!.length];
}

test('rules span the terminal (cols-1), never wrapping, not capped at 80', () => {
  for (const cols of [40, 80, 120, 200, 300]) {
    expect(rules(cols)).toEqual([cols - 1, cols - 1]);
  }
  expect(rules()).toEqual([32, 32]); // unknown width falls back
});
