/**
 * Unit tests for helpers exported from PromptInput.tsx.
 *
 * We only import pure utilities from this module. Side-effect-free top-level
 * code in PromptInput.tsx means this import does not render anything — it
 * just evaluates the module to expose `buildContent`.
 */

import { describe, expect, it } from 'bun:test';
import { buildContent } from './PromptInput.js';
import type { Segment } from '../../../utils/input-editing.js';

// Helpers (mirror those in input-editing.test.ts)
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

describe('buildContent', () => {
  it('preserves leading whitespace in pasted content (the bug fix)', () => {
    // Previously `.replace(/  +/g, ' ').trim()` destroyed indentation in
    // pasted code. Indented pastes must survive verbatim.
    const indented = '    if (x) {\n      return y;\n    }';
    expect(buildContent([paste(indented, 3)])).toBe(indented);
  });

  it('preserves internal consecutive spaces in text segments', () => {
    // Aligned columns (e.g. pasted log lines) must not collapse.
    expect(buildContent([text('foo      bar     baz')])).toBe(
      'foo      bar     baz'
    );
  });

  it('preserves leading/trailing whitespace in text segments', () => {
    expect(buildContent([text('   hello   ')])).toBe('   hello   ');
  });

  it('preserves whitespace inside paste segments surrounded by text', () => {
    const segments: Segment[] = [
      text('prefix '),
      paste('  indented\n    more', 2),
      text(' suffix'),
    ];
    expect(buildContent(segments)).toBe('prefix   indented\n    more suffix');
  });

  it('emits file chip with a single space of padding on each side', () => {
    const segments: Segment[] = [
      text('see'),
      file('/a/b.ts', ''),
      text('please'),
    ];
    expect(buildContent(segments)).toBe('see @file:/a/b.ts please');
  });

  it('preserves user-authored whitespace around a file chip', () => {
    // Chip already emits ` @file:... `; user whitespace adds on top and is
    // preserved verbatim (no collapse).
    const segments: Segment[] = [
      text('see  '),
      file('/a/b.ts', ''),
      text('  please'),
    ];
    expect(buildContent(segments)).toBe('see   @file:/a/b.ts   please');
  });

  it('returns empty string for empty segments list', () => {
    expect(buildContent([])).toBe('');
  });

  it('returns whitespace-only content as-is (caller guards submit)', () => {
    // buildContent no longer trims; submit sites guard via `content.trim()`.
    expect(buildContent([text('   ')])).toBe('   ');
  });

  it('image segments contribute nothing', () => {
    const segments: Segment[] = [
      text('look: '),
      {
        type: 'image',
        base64: 'AAAA',
        mimeType: 'image/png',
        width: 1,
        height: 1,
        sizeBytes: 1,
      },
      text(' thanks'),
    ];
    expect(buildContent(segments)).toBe('look:  thanks');
  });

  it('preserves tab characters inside text segments', () => {
    expect(buildContent([text('\tindented with tab')])).toBe(
      '\tindented with tab'
    );
  });
});
