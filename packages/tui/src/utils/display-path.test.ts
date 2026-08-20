import { describe, expect, test } from 'bun:test';
import { displayBasename } from './display-path.js';

describe('displayBasename', () => {
  test.each([
    ['/workspace/src/App.tsx', 'App.tsx'],
    ['C:\\workspace\\src\\App.tsx', 'App.tsx'],
    ['mixed/path\\App.tsx', 'App.tsx'],
    ['App.tsx', 'App.tsx'],
    ['/workspace/src/', '/workspace/src/'],
  ])('formats %s', (value, expected) => {
    expect(displayBasename(value)).toBe(expected);
  });
});
