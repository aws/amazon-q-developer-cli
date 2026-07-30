import { describe, expect, it } from 'vitest';
import { StaticBuffer } from '../src/renderer/static-buffer.js';

function settle(
  buffer: StaticBuffer,
  previousLines: string[],
  expectedPrefix: string[]
): string[] {
  const copies: number[] = [];
  let previous = previousLines;

  for (const live of ['live-a', 'live-b', 'live-c']) {
    const frame = buffer.compose([live], previous);
    copies.push(frame.copiedPrefixLines);
    expect(frame.lines.slice(0, expectedPrefix.length)).toEqual(expectedPrefix);
    previous = frame.lines;
  }

  expect(copies).toEqual([expectedPrefix.length, expectedPrefix.length, 0]);
  return previous;
}

describe('StaticBuffer frame composition', () => {
  it('reuses resident frame arrays without copying a stable prefix', () => {
    const buffer = new StaticBuffer();
    buffer.append(['static-a', 'static-b']);

    const first = buffer.compose(['live-1'], []);
    const second = buffer.compose(['live-2'], first.lines);
    expect(first.lines).toEqual(['static-a', 'static-b', 'live-1']);

    const third = buffer.compose(['live-3'], second.lines);

    expect(first.copiedPrefixLines).toBe(2);
    expect(second.copiedPrefixLines).toBe(2);
    expect(third.copiedPrefixLines).toBe(0);
    expect(third.lines).toBe(first.lines);
    expect(third.lines).toEqual(['static-a', 'static-b', 'live-3']);
  });

  it('refreshes each resident prefix once after every mutation', () => {
    const buffer = new StaticBuffer();
    buffer.append(['static-a', 'static-b']);
    let previous = settle(buffer, [], ['static-a', 'static-b']);

    buffer.append(['static-c']);
    previous = settle(buffer, previous, ['static-a', 'static-b', 'static-c']);

    buffer.replace(['replacement-a', 'replacement-b']);
    previous = settle(buffer, previous, ['replacement-a', 'replacement-b']);

    buffer.replace(Array.from({ length: 12 }, (_, index) => `trim-${index}`));
    previous = settle(
      buffer,
      previous,
      Array.from({ length: 12 }, (_, index) => `trim-${index}`)
    );
    expect(buffer.trimTo(10)).toBe(true);
    settle(
      buffer,
      previous,
      Array.from({ length: 7 }, (_, index) => `trim-${index + 5}`)
    );
  });

  it('detaches resident frames without mutating the current diff shadow', () => {
    const buffer = new StaticBuffer();
    buffer.append(['static-a', 'static-b']);
    const first = buffer.compose(['live-1'], []);
    const second = buffer.compose(['live-2'], first.lines);

    buffer.clear();

    expect(buffer.view).toEqual([]);
    expect(first.lines).toEqual(['static-a', 'static-b', 'live-1']);
    expect(second.lines).toEqual(['static-a', 'static-b', 'live-2']);

    const afterClear = buffer.compose(['live-new'], second.lines);
    expect(afterClear.lines).not.toBe(first.lines);
    expect(afterClear.lines).not.toBe(second.lines);
    expect(afterClear.lines).toEqual(['live-new']);
  });

  it('detaches resident frames when replacement becomes empty', () => {
    const buffer = new StaticBuffer();
    buffer.append(['static-a', 'static-b']);
    const first = buffer.compose(['live-1'], []);
    const second = buffer.compose(['live-2'], first.lines);

    buffer.replace([]);

    const afterReplace = buffer.compose(['live-new'], second.lines);
    expect(buffer.view).toEqual([]);
    expect(afterReplace.lines).not.toBe(first.lines);
    expect(afterReplace.lines).not.toBe(second.lines);
    expect(afterReplace.lines).toEqual(['live-new']);
    expect(afterReplace.copiedPrefixLines).toBe(0);
  });
});
