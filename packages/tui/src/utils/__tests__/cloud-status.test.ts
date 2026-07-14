import { describe, it, expect } from 'bun:test';
import { formatCloudFooter } from '../cloud-status';
import { ASCII_GLYPHS } from '../glyphs';

describe('formatCloudFooter', () => {
  it('shows Cloud with the bound repo', () => {
    expect(formatCloudFooter('acme/repo')).toBe('Cloud · acme/repo');
  });

  it('shows just "Cloud" for a New empty sandbox (no repo)', () => {
    expect(formatCloudFooter(null)).toBe('Cloud');
    expect(formatCloudFooter(undefined)).toBe('Cloud');
    expect(formatCloudFooter('')).toBe('Cloud');
  });

  it('trims surrounding whitespace and treats blank as no repo', () => {
    expect(formatCloudFooter('  acme/repo  ')).toBe('Cloud · acme/repo');
    expect(formatCloudFooter('   ')).toBe('Cloud');
  });

  it('uses the ASCII small-dot separator when the ASCII glyph set is passed', () => {
    expect(formatCloudFooter('acme/repo', ASCII_GLYPHS)).toBe(
      'Cloud . acme/repo'
    );
  });
});
