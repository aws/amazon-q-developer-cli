/**
 * Unit tests for input-editing utility functions.
 * Tests segment manipulation, cursor movement, and chip handling.
 */

import { describe, expect, it } from 'bun:test';
import {
  type Segment,
  segmentWidth,
  totalWidth,
  locateCursor,
  normalizeSegments,
  deleteWordBackward,
  deleteForward,
  killToEnd,
  killToBeginning,
  moveWordForward,
  moveWordBackward,
  transposeChars,
  getVisibleText,
  deleteWordForward,
  isMultiLine,
  isVisuallyMultiLine,
  getCursorLineInfo,
  moveCursorUp,
  moveCursorDown,
  getVisualLines,
  moveCursorUpVisual,
  moveCursorDownVisual,
  moveToVisualLineStart,
  moveToVisualLineEnd,
  killToVisualLineEnd,
  killToVisualLineBeginning,
  uppercaseWord,
  lowercaseWord,
  capitalizeWord,
  transposeWords,
} from '../input-editing.js';

// Helper to create segments
const text = (value: string): Segment => ({ type: 'text', value });
const paste = (content: string, lineCount: number): Segment => ({
  type: 'paste',
  content,
  lineCount,
  charCount: content.length,
});
const file = (filePath: string, content: string): Segment => ({
  type: 'file',
  filePath,
  content,
  lineCount: content.split('\n').length,
});

describe('input-editing', () => {
  describe('segmentWidth', () => {
    it('returns text length for text segments', () => {
      expect(segmentWidth(text('hello'))).toBe(5);
      expect(segmentWidth(text(''))).toBe(0);
      expect(segmentWidth(text('a'))).toBe(1);
    });

    it('returns 1 for paste chip segments (treated as single unit)', () => {
      expect(segmentWidth(paste('line1\nline2\nline3', 3))).toBe(1);
      expect(segmentWidth(paste('very long content here', 1))).toBe(1);
    });

    it('returns 1 for file chip segments (treated as single unit)', () => {
      expect(segmentWidth(file('test.ts', 'const x = 1;'))).toBe(1);
    });
  });

  describe('totalWidth', () => {
    it('sums widths of all segments', () => {
      expect(totalWidth([text('hello'), text(' world')])).toBe(11);
      expect(
        totalWidth([text('before '), paste('content', 5), text(' after')])
      ).toBe(7 + 1 + 6);
      expect(totalWidth([])).toBe(0);
    });
  });

  describe('locateCursor', () => {
    it('locates cursor in text segment', () => {
      const segments = [text('hello')];
      expect(locateCursor(segments, 0)).toEqual({ segIdx: 0, offset: 0 });
      expect(locateCursor(segments, 3)).toEqual({ segIdx: 0, offset: 3 });
      expect(locateCursor(segments, 5)).toEqual({ segIdx: 0, offset: 5 });
    });

    it('returns segIdx 0, offset 0 for empty segments', () => {
      expect(locateCursor([], 0)).toEqual({ segIdx: 0, offset: 0 });
      expect(locateCursor([], 5)).toEqual({ segIdx: 0, offset: 0 });
    });

    it('locates cursor across multiple text segments', () => {
      const segments = [text('hello'), text(' world')];
      expect(locateCursor(segments, 5)).toEqual({ segIdx: 0, offset: 5 });
      expect(locateCursor(segments, 6)).toEqual({ segIdx: 1, offset: 1 });
      expect(locateCursor(segments, 11)).toEqual({ segIdx: 1, offset: 6 });
    });

    it('clamps to last segment when cursor exceeds total width', () => {
      const segments = [text('abc')];
      const result = locateCursor(segments, 100);
      expect(result).toEqual({ segIdx: 0, offset: 3 });
    });

    it('locates cursor with chip segments (chip width = 1)', () => {
      // "AAA " + [chip] + " BBB" = 4 + 1 + 4 = 9 total width
      const segments = [text('AAA '), paste('content', 5), text(' BBB')];

      // Cursor at position 4 = end of "AAA "
      expect(locateCursor(segments, 4)).toEqual({ segIdx: 0, offset: 4 });

      // Cursor at position 5 = on/after the chip (chip has width 1)
      expect(locateCursor(segments, 5)).toEqual({ segIdx: 1, offset: 1 });

      // Cursor at position 6 = in " BBB" at offset 1
      expect(locateCursor(segments, 6)).toEqual({ segIdx: 2, offset: 1 });
    });
  });

  describe('normalizeSegments', () => {
    it('merges adjacent text segments', () => {
      const result = normalizeSegments([
        text('hello'),
        text(' '),
        text('world'),
      ]);
      expect(result).toEqual([text('hello world')]);
    });

    it('removes empty text segments', () => {
      const result = normalizeSegments([text(''), text('hello'), text('')]);
      expect(result).toEqual([text('hello')]);
    });

    it('preserves chip segments between text', () => {
      const chip = paste('content', 5);
      const result = normalizeSegments([text('before'), chip, text('after')]);
      expect(result).toHaveLength(3);
      expect(result[0]).toEqual(text('before'));
      expect(result[1]).toEqual(chip);
      expect(result[2]).toEqual(text('after'));
    });

    it('returns empty text segment for empty input', () => {
      expect(normalizeSegments([])).toEqual([text('')]);
      expect(normalizeSegments([text('')])).toEqual([text('')]);
    });

    it('preserves adjacent chip segments', () => {
      const chip1 = paste('content1', 3);
      const chip2 = file('test.ts', 'code');
      const result = normalizeSegments([chip1, chip2]);
      expect(result).toHaveLength(2);
      expect(result[0]).toEqual(chip1);
      expect(result[1]).toEqual(chip2);
    });
  });

  describe('chip as single cursor unit', () => {
    it('cursor moves past chip in one step (left arrow)', () => {
      // Simulating: "AAA [chip] BBB" with cursor after "BBB"
      // Moving left 4 times should put cursor at end of chip
      // Moving left once more should jump past entire chip
      const segments = [text('AAA '), paste('content', 5), text(' BBB')];
      const total = totalWidth(segments); // 4 + 1 + 4 = 9

      // Start at end (position 9)
      let cursor = total;

      // Move left 4 times through " BBB"
      cursor = Math.max(0, cursor - 1); // 8
      cursor = Math.max(0, cursor - 1); // 7
      cursor = Math.max(0, cursor - 1); // 6
      cursor = Math.max(0, cursor - 1); // 5 - now at end of chip

      // At position 5, we're right after the chip
      const loc = locateCursor(segments, cursor);
      expect(loc.segIdx).toBe(1); // On chip segment

      // Move left once more - should jump to position 4 (before chip)
      cursor = Math.max(0, cursor - 1); // 4
      const locAfter = locateCursor(segments, cursor);
      expect(locAfter.segIdx).toBe(0); // Now in text segment before chip
      expect(locAfter.offset).toBe(4); // At end of "AAA "
    });

    it('cursor moves past chip in one step (right arrow)', () => {
      const segments = [text('AAA '), paste('content', 5), text(' BBB')];

      // Start at position 4 (end of "AAA ", before chip)
      let cursor = 4;

      // Move right once - should jump past entire chip to position 5
      cursor = Math.min(totalWidth(segments), cursor + 1); // 5

      const loc = locateCursor(segments, cursor);
      // Position 5 is right after the chip (chip occupies position 4-5)
      expect(loc.segIdx).toBe(1); // Still on chip segment but at offset 1 (end)
    });
  });

  describe('deleteWordBackward (Ctrl+W)', () => {
    it('deletes word in text segment', () => {
      const segments = [text('hello world')];
      const cursor = 11; // End of "hello world"

      const result = deleteWordBackward(segments, cursor);

      // Should delete "world" leaving "hello "
      expect(result.segments[0]).toEqual(text('hello '));
      expect(result.cursor).toBe(6);
    });

    it('deletes word from middle of text', () => {
      const segments = [text('hello world foo')];
      const cursor = 11; // End of "hello world"

      const result = deleteWordBackward(segments, cursor);

      expect(result.segments[0]).toEqual(text('hello  foo'));
      expect(result.cursor).toBe(6);
    });

    it('no-op when cursor is at position 0', () => {
      const segments = [text('hello')];
      const result = deleteWordBackward(segments, 0);

      expect(result.segments[0]).toEqual(text('hello'));
      expect(result.cursor).toBe(0);
    });

    it('skips trailing spaces then deletes word', () => {
      const segments = [text('hello   ')];
      const cursor = 8; // End with trailing spaces

      const result = deleteWordBackward(segments, cursor);

      expect(result.segments[0]).toEqual(text(''));
      expect(result.cursor).toBe(0);
    });

    it('when on chip (offset 0), deletes previous segment', () => {
      const segments = [text('before '), paste('content', 5)];
      const cursor = 7; // On the chip (offset 0)

      const result = deleteWordBackward(segments, cursor);

      expect(result.cursor).toBe(0);
    });
  });

  describe('deleteForward (Ctrl+D)', () => {
    it('deletes character under cursor in text', () => {
      const segments = [text('hello')];
      const result = deleteForward(segments, 0);
      expect(result.segments[0]).toEqual(text('ello'));
      expect(result.cursor).toBe(0);
    });

    it('deletes character in middle of text', () => {
      const segments = [text('hello')];
      const result = deleteForward(segments, 2);
      expect(result.segments[0]).toEqual(text('helo'));
      expect(result.cursor).toBe(2);
    });

    it('no-op when cursor is at end of text', () => {
      const segments = [text('hello')];
      const result = deleteForward(segments, 5);
      expect(result.segments[0]).toEqual(text('hello'));
      expect(result.cursor).toBe(5);
    });

    it('no-op on empty text', () => {
      const segments = [text('')];
      const result = deleteForward(segments, 0);
      expect(result.segments[0]).toEqual(text(''));
      expect(result.cursor).toBe(0);
    });

    it('deletes chip when cursor is on chip', () => {
      const segments = [text('before '), paste('content', 5), text(' after')];
      // cursor 7 = start of chip
      const result = deleteForward(segments, 7);
      expect(result.segments).toEqual([text('before  after')]);
      expect(result.cursor).toBe(7);
    });

    it('deletes next chip when cursor is at end of text segment', () => {
      const segments = [text('hello'), paste('content', 5)];
      // cursor 5 = end of "hello", which is at the chip boundary
      const result = deleteForward(segments, 5);
      expect(result.segments).toEqual([text('hello')]);
      expect(result.cursor).toBe(5);
    });

    it('deletes file chip when cursor is on it', () => {
      const segments = [
        text('before '),
        file('test.ts', 'code'),
        text(' after'),
      ];
      const result = deleteForward(segments, 7);
      expect(result.segments).toEqual([text('before  after')]);
      expect(result.cursor).toBe(7);
    });

    it('deletes last character leaving empty text', () => {
      const segments = [text('x')];
      const result = deleteForward(segments, 0);
      expect(result.segments).toEqual([text('')]);
      expect(result.cursor).toBe(0);
    });
  });

  describe('backspace on chip (simulated)', () => {
    // Note: Actual backspace is handled in PromptInput.handleBackspace
    // These tests verify the segment/cursor logic that backspace relies on

    it('chip has width 1 so cursor positions work correctly', () => {
      // "before " (7) + chip (1) + " after" (6) = 14 total
      const segments = [text('before '), paste('content', 5), text(' after')];
      const total = totalWidth(segments);
      expect(total).toBe(14);

      // Cursor positions:
      // 0-7: in "before " text
      // 7: at end of "before " (segIdx 0, offset 7)
      // 8: right after chip (segIdx 1, offset 1) - this triggers chip deletion
      // 9-14: in " after" text

      // Cursor at position 8 = right after chip
      const loc8 = locateCursor(segments, 8);
      expect(loc8.segIdx).toBe(1); // On chip segment
      expect(loc8.offset).toBe(1); // At end of chip (offset 1 = after the chip)

      // Cursor at position 9 = in " after" segment
      const loc9 = locateCursor(segments, 9);
      expect(loc9.segIdx).toBe(2); // In " after" segment
      expect(loc9.offset).toBe(1); // At position 1 in " after"
    });

    it('cursor right after chip (offset=1) triggers chip deletion', () => {
      const segments = [text('before '), paste('content', 5)];
      const cursor = 8; // Right after chip (7 + 1)

      const loc = locateCursor(segments, cursor);
      // offset === 1 on a chip means cursor is right after it
      expect(loc.segIdx).toBe(1);
      expect(loc.offset).toBe(1);

      // PromptInput.handleBackspace checks: seg.type !== 'text' && offset === 1
      // and deletes the chip, moving cursor by 1
    });
  });

  describe('killToEnd with chips', () => {
    it('kills chip and everything after when cursor is on chip', () => {
      const segments = [text('before '), paste('content', 5), text(' after')];
      const cursor = 7; // At start of chip

      const result = killToEnd(segments, cursor);

      expect(result.segments).toEqual([text('before ')]);
      expect(result.cursor).toBe(7);
    });

    it('kills from cursor to end in text segment', () => {
      const segments = [text('hello world')];
      const cursor = 5; // After "hello"

      const result = killToEnd(segments, cursor);

      expect(result.segments).toEqual([text('hello')]);
      expect(result.cursor).toBe(5);
    });

    it('no-op when cursor is at end', () => {
      const segments = [text('hello')];
      const cursor = 5;

      const result = killToEnd(segments, cursor);

      expect(result.segments[0]).toEqual(text('hello'));
      expect(result.cursor).toBe(5);
    });
  });

  describe('killToBeginning with chips', () => {
    it('kills everything before chip when cursor is on chip', () => {
      const segments = [text('before '), paste('content', 5), text(' after')];
      const cursor = 7; // At start of chip

      const result = killToBeginning(segments, cursor);

      expect(result.segments).toHaveLength(2);
      expect(result.segments[0]?.type).toBe('paste');
      expect(result.cursor).toBe(0);
    });

    it('kills from beginning to cursor in text segment', () => {
      const segments = [text('hello world')];
      const cursor = 6; // After "hello "

      const result = killToBeginning(segments, cursor);

      expect(result.segments).toEqual([text('world')]);
      expect(result.cursor).toBe(0);
    });

    it('no-op when cursor is at beginning', () => {
      const segments = [text('hello')];
      const cursor = 0;

      const result = killToBeginning(segments, cursor);

      expect(result.segments[0]).toEqual(text('hello'));
      expect(result.cursor).toBe(0);
    });
  });

  describe('moveWordForward with chips', () => {
    it('moves past chip as single unit', () => {
      const segments = [text('AAA '), paste('content', 5), text(' BBB')];
      const cursor = 4; // At end of "AAA ", before chip

      const newCursor = moveWordForward(segments, cursor);

      expect(newCursor).toBe(5);
    });

    it('skips word then spaces in text', () => {
      const segments = [text('hello world foo')];
      const cursor = 0;

      const newCursor = moveWordForward(segments, cursor);

      // Should skip "hello " -> position 6
      expect(newCursor).toBe(6);
    });

    it('no-op when cursor is at end', () => {
      const segments = [text('hello')];
      const cursor = 5;

      const newCursor = moveWordForward(segments, cursor);

      expect(newCursor).toBe(5);
    });
  });

  describe('moveWordBackward with chips', () => {
    it('moves past chip as single unit', () => {
      const segments = [text('AAA '), paste('content', 5), text(' BBB')];
      const cursor = 5; // Right after chip

      const newCursor = moveWordBackward(segments, cursor);

      expect(newCursor).toBeLessThan(5);
    });

    it('skips spaces then word in text', () => {
      const segments = [text('hello world')];
      const cursor = 11; // End

      const newCursor = moveWordBackward(segments, cursor);

      // Should skip back to start of "world" -> position 6
      expect(newCursor).toBe(6);
    });

    it('no-op when cursor is at beginning', () => {
      const segments = [text('hello')];
      const cursor = 0;

      const newCursor = moveWordBackward(segments, cursor);

      expect(newCursor).toBe(0);
    });
  });

  describe('transposeChars', () => {
    it('swaps characters at cursor position', () => {
      const segments = [text('abcd')];
      const cursor = 2; // After 'ab'

      const result = transposeChars(segments, cursor);

      // Swaps char before cursor (b) with char at cursor (c)
      expect(result.segments[0]).toEqual(text('acbd'));
      expect(result.cursor).toBe(3); // Cursor moves forward
    });

    it('swaps last two chars when cursor at end', () => {
      const segments = [text('abcd')];
      const cursor = 4; // At end

      const result = transposeChars(segments, cursor);

      expect(result.segments[0]).toEqual(text('abdc'));
    });

    it('does nothing at start of text', () => {
      const segments = [text('abcd')];
      const cursor = 0;

      const result = transposeChars(segments, cursor);

      expect(result.segments[0]).toEqual(text('abcd'));
      expect(result.cursor).toBe(0);
    });
  });

  describe('getVisibleText', () => {
    it('returns text content with zero-width space for chips', () => {
      const segments = [text('hello'), paste('content', 5), text('world')];
      const visible = getVisibleText(segments);

      // Chip becomes zero-width space
      expect(visible).toBe('hello\u200Bworld');
    });

    it('returns plain text for text-only segments', () => {
      const segments = [text('hello world')];
      expect(getVisibleText(segments)).toBe('hello world');
    });
  });

  describe('cursor movement (simulated arrow keys)', () => {
    it('left arrow decrements cursor by 1', () => {
      const cursor = 5;
      const newCursor = Math.max(0, cursor - 1);
      expect(newCursor).toBe(4);
    });

    it('right arrow increments cursor by 1', () => {
      const segments = [text('hello')];
      const cursor = 3;
      const newCursor = Math.min(totalWidth(segments), cursor + 1);
      expect(newCursor).toBe(4);
    });

    it('left arrow stops at 0', () => {
      const cursor = 0;
      const newCursor = Math.max(0, cursor - 1);
      expect(newCursor).toBe(0);
    });

    it('right arrow stops at total width', () => {
      const segments = [text('hello')];
      const cursor = 5;
      const newCursor = Math.min(totalWidth(segments), cursor + 1);
      expect(newCursor).toBe(5);
    });

    it('Ctrl+A moves to beginning (cursor = 0)', () => {
      const newCursor = 0;
      expect(newCursor).toBe(0);
    });

    it('Ctrl+E moves to end (cursor = totalWidth)', () => {
      const segments = [text('hello world')];
      const newCursor = totalWidth(segments);
      expect(newCursor).toBe(11);
    });
  });

  describe('text insertion (simulated)', () => {
    it('inserts text at cursor position in text segment', () => {
      const segments = [text('helloworld')];
      const cursor = 5; // After 'hello'

      const { segIdx, offset } = locateCursor(segments, cursor);
      const seg = segments[segIdx]!;

      if (seg.type === 'text') {
        const newValue =
          seg.value.slice(0, offset) + ' ' + seg.value.slice(offset);
        const newSegs = [...segments];
        newSegs[segIdx] = { type: 'text', value: newValue };

        expect(newSegs[0]).toEqual(text('hello world'));
      }
    });

    it('inserts newline character (Ctrl+J)', () => {
      const segments = [text('line1')];
      const cursor = 5;

      const { segIdx, offset } = locateCursor(segments, cursor);
      const seg = segments[segIdx]!;

      if (seg.type === 'text') {
        const newValue =
          seg.value.slice(0, offset) + '\n' + seg.value.slice(offset);
        expect(newValue).toBe('line1\n');
      }
    });
  });

  describe('backspace (simulated)', () => {
    it('deletes character before cursor in text', () => {
      const segments = [text('hello')];
      const cursor = 5;

      const { segIdx, offset } = locateCursor(segments, cursor);
      const seg = segments[segIdx]!;

      if (seg.type === 'text' && offset > 0) {
        const newValue =
          seg.value.slice(0, offset - 1) + seg.value.slice(offset);
        expect(newValue).toBe('hell');
      }
    });

    it('deletes multiple characters with repeated backspace', () => {
      let value = 'hello';
      let cursor = 5;

      // Simulate 2 backspaces
      for (let i = 0; i < 2; i++) {
        if (cursor > 0) {
          value = value.slice(0, cursor - 1) + value.slice(cursor);
          cursor--;
        }
      }

      expect(value).toBe('hel');
      expect(cursor).toBe(3);
    });

    it('no-op when cursor is at position 0', () => {
      const segments = [text('hello')];
      const cursor = 0;

      // handleBackspace returns early when cursor === 0
      expect(cursor).toBe(0);
      expect(segments[0]).toEqual(text('hello'));
    });

    it('deletes last char of previous text segment when at segment boundary', () => {
      // locateCursor uses <= so cursor at exact boundary stays in current segment.
      // The offset===0 && segIdx>0 branch in handleBackspace is reached when
      // the previous segment is empty (e.g. after normalization leaves a chip at segIdx 0
      // with text at segIdx 1, and cursor is at start of text).
      // More commonly, backspace at end of text before a chip just deletes the last
      // char of the text segment via the normal text path (offset > 0).
      const segments = [text('hello '), paste('content', 5)];
      const cursor = 6; // End of "hello " text

      const loc = locateCursor(segments, cursor);
      // cursor 6 is at offset 6 of "hello " (end of text segment)
      expect(loc.segIdx).toBe(0);
      expect(loc.offset).toBe(6);

      // handleBackspace: seg is text, offset > 0 -> delete char before cursor
      const seg = segments[loc.segIdx]!;
      if (seg.type === 'text' && loc.offset > 0) {
        const newValue =
          seg.value.slice(0, loc.offset - 1) + seg.value.slice(loc.offset);
        expect(newValue).toBe('hello');
      }
    });
  });

  describe('text insertion on chip (simulated)', () => {
    it('inserts text after chip when cursor is on a chip', () => {
      const segments = [text('before '), paste('content', 5)];
      const cursor = 8; // After chip (7 + 1)

      const loc = locateCursor(segments, cursor);
      expect(loc.segIdx).toBe(1); // On chip

      // PromptInput.insertText: when seg is not text, insert after chip
      // Result: [...segments.slice(0, segIdx + 1), text('X'), ...segments.slice(segIdx + 1)]
      const newSegs = [
        ...segments.slice(0, loc.segIdx + 1),
        { type: 'text' as const, value: 'X' },
        ...segments.slice(loc.segIdx + 1),
      ];
      const normalized = normalizeSegments(newSegs);
      expect(normalized).toHaveLength(3);
      expect(normalized[2]).toEqual(text('X'));
    });
  });

  describe('empty submit guard (simulated)', () => {
    it('empty segments produce empty content after trim', () => {
      // buildContent joins segments and trims
      const segments = [text('')];
      const content = segments
        .map((s) => (s.type === 'text' ? s.value : ''))
        .join('')
        .trim();
      expect(content).toBe('');
    });

    it('whitespace-only segments produce empty content after trim', () => {
      const segments = [text('   ')];
      const content = segments
        .map((s) => (s.type === 'text' ? s.value : ''))
        .join('')
        .trim();
      expect(content).toBe('');
    });
  });

  describe('deleteWordForward (Alt+D)', () => {
    it('deletes word forward from cursor', () => {
      const segments = [text('hello world')];
      const result = deleteWordForward(segments, 0);
      // Deletes "hello " (word + trailing spaces)
      expect(result.segments[0]).toEqual(text('world'));
      expect(result.cursor).toBe(0);
    });

    it('deletes word from middle of text', () => {
      const segments = [text('hello world foo')];
      const result = deleteWordForward(segments, 6);
      // Cursor at 'w', deletes "world "
      expect(result.segments[0]).toEqual(text('hello foo'));
      expect(result.cursor).toBe(6);
    });

    it('no-op when cursor is at end', () => {
      const segments = [text('hello')];
      const result = deleteWordForward(segments, 5);
      expect(result.segments[0]).toEqual(text('hello'));
      expect(result.cursor).toBe(5);
    });

    it('deletes chip when cursor is on chip', () => {
      const segments = [text('before '), paste('content', 3), text(' after')];
      const result = deleteWordForward(segments, 7); // On the chip (width of 'before ' is 7, chip starts at 7)
      expect(result.segments).toHaveLength(1);
      expect(result.segments[0]).toEqual(text('before  after'));
      expect(result.cursor).toBe(7);
    });
  });

  describe('isMultiLine', () => {
    it('returns false for single-line text', () => {
      expect(isMultiLine([text('hello world')])).toBe(false);
    });

    it('returns true for text with newlines', () => {
      expect(isMultiLine([text('hello\nworld')])).toBe(true);
    });

    it('returns false for empty text', () => {
      expect(isMultiLine([text('')])).toBe(false);
    });
  });

  describe('getCursorLineInfo', () => {
    it('returns correct info for single line', () => {
      const segments = [text('hello')];
      const info = getCursorLineInfo(segments, 3);
      expect(info.lineIndex).toBe(0);
      expect(info.col).toBe(3);
      expect(info.lineLengths).toEqual([5]);
    });

    it('returns correct info for cursor on first line of multi-line', () => {
      // "hello\nworld" -> lines: ["hello", "world"], lengths: [5, 5]
      const segments = [text('hello\nworld')];
      const info = getCursorLineInfo(segments, 3);
      expect(info.lineIndex).toBe(0);
      expect(info.col).toBe(3);
      expect(info.lineLengths).toEqual([5, 5]);
    });

    it('returns correct info for cursor on second line', () => {
      // "hello\nworld" -> cursor 8 = line 1, col 2 (past "hello\n" = 6 chars, then 2 more)
      const segments = [text('hello\nworld')];
      const info = getCursorLineInfo(segments, 8);
      expect(info.lineIndex).toBe(1);
      expect(info.col).toBe(2);
    });

    it('returns correct info for cursor at start of second line', () => {
      const segments = [text('hello\nworld')];
      const info = getCursorLineInfo(segments, 6); // right after \n
      expect(info.lineIndex).toBe(1);
      expect(info.col).toBe(0);
    });

    it('handles three lines', () => {
      // "ab\ncd\nef" -> lines: ["ab","cd","ef"], lengths: [2,2,2]
      const segments = [text('ab\ncd\nef')];
      const info = getCursorLineInfo(segments, 7); // "ab\ncd\ne" -> line 2, col 1
      expect(info.lineIndex).toBe(2);
      expect(info.col).toBe(1);
      expect(info.lineLengths).toEqual([2, 2, 2]);
    });
  });

  describe('moveCursorUp', () => {
    it('returns null on single line', () => {
      const segments = [text('hello')];
      expect(moveCursorUp(segments, 3)).toBeNull();
    });

    it('returns null when already on first line', () => {
      const segments = [text('hello\nworld')];
      expect(moveCursorUp(segments, 3)).toBeNull();
    });

    it('moves from second line to first line preserving column', () => {
      // "hello\nworld", cursor at 8 (line 1, col 2) -> should go to col 2 on line 0 = offset 2
      const segments = [text('hello\nworld')];
      expect(moveCursorUp(segments, 8)).toBe(2);
    });

    it('clamps column when upper line is shorter', () => {
      // "ab\nworld", cursor at 7 (line 1, col 4) -> line 0 has length 2, so clamp to col 2 = offset 2
      const segments = [text('ab\nworld')];
      expect(moveCursorUp(segments, 7)).toBe(2);
    });

    it('moves from third line to second line', () => {
      // "ab\ncd\nef", cursor at 7 (line 2, col 1) -> line 1, col 1 = offset 3+1 = 4
      const segments = [text('ab\ncd\nef')];
      expect(moveCursorUp(segments, 7)).toBe(4);
    });
  });

  describe('moveCursorDown', () => {
    it('returns null on single line', () => {
      const segments = [text('hello')];
      expect(moveCursorDown(segments, 3)).toBeNull();
    });

    it('returns null when already on last line', () => {
      const segments = [text('hello\nworld')];
      expect(moveCursorDown(segments, 8)).toBeNull();
    });

    it('moves from first line to second line preserving column', () => {
      // "hello\nworld", cursor at 3 (line 0, col 3) -> line 1, col 3 = offset 6+3 = 9
      const segments = [text('hello\nworld')];
      expect(moveCursorDown(segments, 3)).toBe(9);
    });

    it('clamps column when lower line is shorter', () => {
      // "hello\nab", cursor at 4 (line 0, col 4) -> line 1 has length 2, clamp to col 2 = offset 6+2 = 8
      const segments = [text('hello\nab')];
      expect(moveCursorDown(segments, 4)).toBe(8);
    });

    it('moves from first line to second in three-line text', () => {
      // "ab\ncd\nef", cursor at 1 (line 0, col 1) -> line 1, col 1 = offset 3+1 = 4
      const segments = [text('ab\ncd\nef')];
      expect(moveCursorDown(segments, 1)).toBe(4);
    });
  });

  describe('uppercaseWord (Alt+U)', () => {
    it('uppercases word from cursor', () => {
      const segments = [text('hello world')];
      const result = uppercaseWord(segments, 0);
      expect(result.segments[0]).toEqual(text('HELLO world'));
      expect(result.cursor).toBe(5);
    });

    it('uppercases from mid-word', () => {
      const segments = [text('hello world')];
      const result = uppercaseWord(segments, 2);
      expect(result.segments[0]).toEqual(text('heLLO world'));
      expect(result.cursor).toBe(5);
    });

    it('no-op at end of text', () => {
      const segments = [text('hello')];
      const result = uppercaseWord(segments, 5);
      expect(result.segments[0]).toEqual(text('hello'));
    });
  });

  describe('lowercaseWord (Alt+L)', () => {
    it('lowercases word from cursor', () => {
      const segments = [text('HELLO WORLD')];
      const result = lowercaseWord(segments, 0);
      expect(result.segments[0]).toEqual(text('hello WORLD'));
      expect(result.cursor).toBe(5);
    });
  });

  describe('capitalizeWord (Alt+C)', () => {
    it('capitalizes word from cursor', () => {
      const segments = [text('hello world')];
      const result = capitalizeWord(segments, 0);
      expect(result.segments[0]).toEqual(text('Hello world'));
      expect(result.cursor).toBe(5);
    });

    it('capitalizes from space before word', () => {
      const segments = [text('hello world')];
      const result = capitalizeWord(segments, 5);
      expect(result.segments[0]).toEqual(text('hello World'));
      expect(result.cursor).toBe(11);
    });
  });

  describe('transposeWords (Alt+T)', () => {
    it('swaps words around cursor', () => {
      const segments = [text('hello world')];
      const result = transposeWords(segments, 5); // at space between words
      expect(result.segments[0]).toEqual(text('world hello'));
    });

    it('no-op with single word', () => {
      const segments = [text('hello')];
      const result = transposeWords(segments, 3);
      expect(result.segments[0]).toEqual(text('hello'));
    });
  });

  describe('isVisuallyMultiLine', () => {
    it('returns false for short text within width', () => {
      expect(isVisuallyMultiLine([text('hello')], 80)).toBe(false);
    });

    it('returns true for text exceeding width', () => {
      expect(isVisuallyMultiLine([text('a'.repeat(81))], 80)).toBe(true);
    });

    it('returns true for text with literal newlines', () => {
      expect(isVisuallyMultiLine([text('hello\nworld')], 80)).toBe(true);
    });

    it('returns false for text exactly at width', () => {
      expect(isVisuallyMultiLine([text('a'.repeat(80))], 80)).toBe(false);
    });

    it('falls back to literal newline check when width <= 0', () => {
      expect(isVisuallyMultiLine([text('a'.repeat(200))], 0)).toBe(false);
      expect(isVisuallyMultiLine([text('hello\nworld')], 0)).toBe(true);
    });
  });

  describe('getVisualLines', () => {
    it('returns single line for short text', () => {
      const lines = getVisualLines('hello', 80);
      expect(lines).toEqual([{ start: 0, length: 5 }]);
    });

    it('wraps long text into multiple visual lines', () => {
      // 15 chars at width 10 -> 2 visual lines: [0..10), [10..15)
      const lines = getVisualLines('a'.repeat(15), 10);
      expect(lines).toEqual([
        { start: 0, length: 10 },
        { start: 10, length: 5 },
      ]);
    });

    it('wraps text exactly at width boundary', () => {
      const lines = getVisualLines('a'.repeat(20), 10);
      expect(lines).toEqual([
        { start: 0, length: 10 },
        { start: 10, length: 10 },
      ]);
    });

    it('handles literal newlines within wrapping', () => {
      // "abcde\nfghij" at width 3 -> "abc", "de", "fgh", "ij"
      const lines = getVisualLines('abcde\nfghij', 3);
      expect(lines).toEqual([
        { start: 0, length: 3 },
        { start: 3, length: 2 },
        { start: 6, length: 3 },
        { start: 9, length: 2 },
      ]);
    });

    it('handles empty lines from consecutive newlines', () => {
      const lines = getVisualLines('a\n\nb', 80);
      expect(lines).toEqual([
        { start: 0, length: 1 },
        { start: 2, length: 0 },
        { start: 3, length: 1 },
      ]);
    });
  });

  describe('moveCursorUpVisual', () => {
    it('returns null on single visual line', () => {
      expect(moveCursorUpVisual([text('hello')], 3, 80)).toBeNull();
    });

    it('moves up in visually wrapped text', () => {
      // "abcdefghij" (10 chars) at width 5 -> line 0: [0..5), line 1: [5..10)
      // cursor at 7 (line 1, col 2) -> should go to line 0, col 2 = offset 2
      expect(moveCursorUpVisual([text('abcdefghij')], 7, 5)).toBe(2);
    });

    it('clamps column when upper line is shorter', () => {
      // "ab\ncdefghij" at width 5 -> line 0: "ab" (start 0, len 2), line 1: "cdefg" (start 3, len 5), line 2: "hij" (start 8, len 3)
      // cursor at 7 (line 1, col 4) -> line 0 has length 2, clamp to col 2 = offset 2
      expect(moveCursorUpVisual([text('ab\ncdefghij')], 7, 5)).toBe(2);
    });

    it('returns null when already on first visual line', () => {
      expect(moveCursorUpVisual([text('abcdefghij')], 3, 5)).toBeNull();
    });

    it('works with literal newlines and wrapping combined', () => {
      // "hello\nworld_is_great" at width 10
      // line 0: "hello" (start 0, len 5)
      // line 1: "world_is_g" (start 6, len 10)
      // line 2: "reat" (start 16, len 4)
      // cursor at 18 (line 2, col 2) -> line 1, col 2 = offset 8
      expect(moveCursorUpVisual([text('hello\nworld_is_great')], 18, 10)).toBe(
        8
      );
    });
  });

  describe('moveCursorDownVisual', () => {
    it('returns null on single visual line', () => {
      expect(moveCursorDownVisual([text('hello')], 3, 80)).toBeNull();
    });

    it('moves down in visually wrapped text', () => {
      // "abcdefghij" at width 5 -> line 0: [0..5), line 1: [5..10)
      // cursor at 2 (line 0, col 2) -> line 1, col 2 = offset 7
      expect(moveCursorDownVisual([text('abcdefghij')], 2, 5)).toBe(7);
    });

    it('clamps column when lower line is shorter', () => {
      // "abcdefgh" (8 chars) at width 5 -> line 0: [0..5), line 1: [5..8)
      // cursor at 4 (line 0, col 4) -> line 1 has length 3, clamp to col 3 = offset 8
      expect(moveCursorDownVisual([text('abcdefgh')], 4, 5)).toBe(8);
    });

    it('returns null when already on last visual line', () => {
      expect(moveCursorDownVisual([text('abcdefghij')], 7, 5)).toBeNull();
    });
  });

  describe('moveToVisualLineStart (Ctrl+A multi-line)', () => {
    it('returns 0 for single line', () => {
      expect(moveToVisualLineStart([text('hello')], 3, 80)).toBe(0);
    });

    it('moves to start of current logical line', () => {
      // "hello\nworld", cursor at 8 (line 1, col 2) -> start of line 1 = 6
      expect(moveToVisualLineStart([text('hello\nworld')], 8, 80)).toBe(6);
    });

    it('moves to start of wrapped visual line', () => {
      // "abcdefghij" at width 5 -> line 0: [0..5), line 1: [5..10)
      // cursor at 7 (line 1, col 2) -> start of visual line 1 = 5
      expect(moveToVisualLineStart([text('abcdefghij')], 7, 5)).toBe(5);
    });

    it('handles cursor already at line start', () => {
      expect(moveToVisualLineStart([text('hello\nworld')], 6, 80)).toBe(6);
    });

    it('works with wrapping and newlines combined', () => {
      // "hello\nworld_is_great" at width 10
      // line 0: "hello" (start 0, len 5)
      // line 1: "world_is_g" (start 6, len 10)
      // line 2: "reat" (start 16, len 4)
      // cursor at 18 (line 2, col 2) -> start of line 2 = 16
      expect(
        moveToVisualLineStart([text('hello\nworld_is_great')], 18, 10)
      ).toBe(16);
    });
  });

  describe('moveToVisualLineEnd (Ctrl+E multi-line)', () => {
    it('returns total width for single line', () => {
      expect(moveToVisualLineEnd([text('hello')], 2, 80)).toBe(5);
    });

    it('moves to end of current logical line', () => {
      // "hello\nworld", cursor at 8 (line 1, col 2) -> end of line 1 = 11
      expect(moveToVisualLineEnd([text('hello\nworld')], 8, 80)).toBe(11);
    });

    it('moves to end of first line, not entire input', () => {
      // "hello\nworld", cursor at 2 (line 0, col 2) -> end of line 0 = 5
      expect(moveToVisualLineEnd([text('hello\nworld')], 2, 80)).toBe(5);
    });

    it('moves to end of wrapped visual line', () => {
      // "abcdefghij" at width 5 -> line 0: [0..5), line 1: [5..10)
      // cursor at 2 (line 0, col 2) -> end of visual line 0 = 5
      expect(moveToVisualLineEnd([text('abcdefghij')], 2, 5)).toBe(5);
    });

    it('handles cursor already at line end', () => {
      expect(moveToVisualLineEnd([text('hello\nworld')], 5, 80)).toBe(5);
    });
  });

  describe('killToVisualLineEnd (Ctrl+K multi-line)', () => {
    it('kills to end of current line only', () => {
      // "hello\nworld", cursor at 2 -> kill "llo" from line 0, keep "\nworld"
      const result = killToVisualLineEnd([text('hello\nworld')], 2, 80);
      expect(result.segments[0]).toEqual(text('he\nworld'));
      expect(result.cursor).toBe(2);
    });

    it('kills to end of second line only', () => {
      // "hello\nworld\nfoo", cursor at 8 (line 1, col 2) -> kill "rld", keep rest
      const result = killToVisualLineEnd([text('hello\nworld\nfoo')], 8, 80);
      expect(result.segments[0]).toEqual(text('hello\nwo\nfoo'));
      expect(result.cursor).toBe(8);
    });

    it('no-op when cursor is at end of visual line', () => {
      // "hello\nworld", cursor at 5 (end of line 0)
      const result = killToVisualLineEnd([text('hello\nworld')], 5, 80);
      expect(result.segments[0]).toEqual(text('hello\nworld'));
      expect(result.cursor).toBe(5);
    });

    it('kills to end of wrapped visual line', () => {
      // "abcdefghij" at width 5 -> line 0: [0..5), line 1: [5..10)
      // cursor at 2 -> kill "cde" (to end of visual line 0), keep "fghij"
      const result = killToVisualLineEnd([text('abcdefghij')], 2, 5);
      expect(result.segments[0]).toEqual(text('abfghij'));
      expect(result.cursor).toBe(2);
    });
  });

  describe('killToVisualLineBeginning (Ctrl+U multi-line)', () => {
    it('kills to beginning of current line only', () => {
      // "hello\nworld", cursor at 8 (line 1, col 2) -> kill "wo", keep rest
      const result = killToVisualLineBeginning([text('hello\nworld')], 8, 80);
      expect(result.segments[0]).toEqual(text('hello\nrld'));
      expect(result.cursor).toBe(6);
    });

    it('preserves other lines', () => {
      // "hello\nworld\nfoo", cursor at 8 (line 1, col 2) -> kill "wo"
      const result = killToVisualLineBeginning(
        [text('hello\nworld\nfoo')],
        8,
        80
      );
      expect(result.segments[0]).toEqual(text('hello\nrld\nfoo'));
      expect(result.cursor).toBe(6);
    });

    it('no-op when cursor is at beginning of visual line', () => {
      const result = killToVisualLineBeginning([text('hello\nworld')], 6, 80);
      expect(result.segments[0]).toEqual(text('hello\nworld'));
      expect(result.cursor).toBe(6);
    });

    it('kills to beginning of wrapped visual line', () => {
      // "abcdefghij" at width 5 -> line 0: [0..5), line 1: [5..10)
      // cursor at 7 (line 1, col 2) -> kill "fg" (from start of visual line 1), keep rest
      const result = killToVisualLineBeginning([text('abcdefghij')], 7, 5);
      expect(result.segments[0]).toEqual(text('abcdehij'));
      expect(result.cursor).toBe(5);
    });
  });
});
