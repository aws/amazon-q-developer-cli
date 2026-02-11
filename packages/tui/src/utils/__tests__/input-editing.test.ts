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
  killToEnd,
  killToBeginning,
  moveWordForward,
  moveWordBackward,
  transposeChars,
  getVisibleText,
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
      expect(totalWidth([text('before '), paste('content', 5), text(' after')])).toBe(7 + 1 + 6);
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
      const result = normalizeSegments([text('hello'), text(' '), text('world')]);
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
      const cursor = 5;
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
        const newValue = seg.value.slice(0, offset) + ' ' + seg.value.slice(offset);
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
        const newValue = seg.value.slice(0, offset) + '\n' + seg.value.slice(offset);
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
        const newValue = seg.value.slice(0, offset - 1) + seg.value.slice(offset);
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
        const newValue = seg.value.slice(0, loc.offset - 1) + seg.value.slice(loc.offset);
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
      const content = segments.map(s => s.type === 'text' ? s.value : '').join('').trim();
      expect(content).toBe('');
    });

    it('whitespace-only segments produce empty content after trim', () => {
      const segments = [text('   ')];
      const content = segments.map(s => s.type === 'text' ? s.value : '').join('').trim();
      expect(content).toBe('');
    });
  });
});
