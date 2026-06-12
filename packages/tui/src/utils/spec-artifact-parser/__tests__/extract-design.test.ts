import { describe, it, expect } from 'bun:test';
import { extractDesign } from '../extract-design';
import { DESIGN_OVERVIEW_MAX_CHARS } from '../types';

describe('extractDesign', () => {
  it('returns empty fields for empty input', () => {
    const out = extractDesign('');
    expect(out.overview).toBe('');
    expect(out.overviewTruncated).toBe(false);
    expect(out.sections).toEqual([]);
  });

  it('returns empty fields for whitespace-only input', () => {
    const out = extractDesign('  \n  \n');
    expect(out.overview).toBe('');
    expect(out.sections).toEqual([]);
  });

  describe('overview fallback chain', () => {
    it('uses Introduction when present', () => {
      const md = [
        '# Design',
        '',
        '## Introduction',
        'Intro paragraph one.',
        '',
        '## Architecture Overview',
        'Arch paragraph here.',
      ].join('\n');
      const out = extractDesign(md);
      expect(out.overview).toBe('Intro paragraph one.');
    });

    it('uses the Overview heading body, not the raw "## Overview" line', () => {
      const md = [
        '# Design',
        '',
        '## Overview',
        '',
        'This design covers the X system.',
        '',
        '## Architecture',
        'arch body',
      ].join('\n');
      const out = extractDesign(md);
      expect(out.overview).toBe('This design covers the X system.');
    });

    it('fallback skips heading lines (never returns a bare heading)', () => {
      // No recognised overview heading; the first H2 is a custom one. The
      // overview must be its prose, not the "## Custom Section" heading line.
      const md = [
        '# Design',
        '',
        '## Custom Section',
        '',
        'Prose under the custom section.',
        '',
      ].join('\n');
      const out = extractDesign(md);
      expect(out.overview).toBe('Prose under the custom section.');
    });

    it('falls back to Architecture Overview when Introduction is missing', () => {
      const md = [
        '# Design',
        '',
        '## Architecture Overview',
        'Arch paragraph here.',
        '',
        '## Other',
        'Other body.',
      ].join('\n');
      const out = extractDesign(md);
      expect(out.overview).toBe('Arch paragraph here.');
    });

    it('falls back to Architecture Overview when Introduction is empty', () => {
      const md = [
        '# Design',
        '',
        '## Introduction',
        '',
        '## Architecture Overview',
        'Arch paragraph.',
      ].join('\n');
      const out = extractDesign(md);
      expect(out.overview).toBe('Arch paragraph.');
    });

    it('falls back to first paragraph when no named sections', () => {
      const md = [
        '# Design',
        '',
        'First paragraph here.',
        'Continuation of first paragraph.',
        '',
        'Second paragraph excluded.',
      ].join('\n');
      const out = extractDesign(md);
      expect(out.overview).toBe(
        'First paragraph here. Continuation of first paragraph.'
      );
    });

    it('returns empty overview when no recognisable content', () => {
      const md = '# Design\n\n';
      const out = extractDesign(md);
      expect(out.overview).toBe('');
    });
  });

  it('truncates long overviews and sets overviewTruncated', () => {
    const longLine = 'x'.repeat(DESIGN_OVERVIEW_MAX_CHARS + 100);
    const md = ['# Design', '', '## Introduction', longLine].join('\n');
    const out = extractDesign(md);
    expect(out.overviewTruncated).toBe(true);
    expect(out.overview.length).toBe(DESIGN_OVERVIEW_MAX_CHARS + 1); // +1 for the ellipsis char
    expect(out.overview.endsWith('…')).toBe(true);
  });

  it('does not truncate short overviews', () => {
    const md = ['# Design', '', '## Introduction', 'Short intro.'].join('\n');
    const out = extractDesign(md);
    expect(out.overviewTruncated).toBe(false);
    expect(out.overview).toBe('Short intro.');
  });

  describe('sections', () => {
    it('returns H2 sections in source order', () => {
      const md = [
        '# Design',
        '',
        '## Beta',
        'beta body',
        '',
        '## Alpha',
        'alpha body',
        '',
        '## Gamma',
        'gamma body',
      ].join('\n');
      const out = extractDesign(md);
      expect(out.sections.map((s) => s.title)).toEqual([
        'Beta',
        'Alpha',
        'Gamma',
      ]);
    });

    it('skips empty sections', () => {
      const md = ['# Design', '', '## Empty', '', '## Filled', 'has body'].join(
        '\n'
      );
      const out = extractDesign(md);
      expect(out.sections.map((s) => s.title)).toEqual(['Filled']);
    });

    it('excludes H1 and H3+ headings', () => {
      const md = [
        '# Design',
        '',
        '## Real H2',
        'body',
        '',
        '### Nested H3',
        'h3 body',
        '',
        '#### Nested H4',
        'h4 body',
      ].join('\n');
      const out = extractDesign(md);
      expect(out.sections.map((s) => s.title)).toEqual(['Real H2']);
    });

    it('detail body slices verbatim from heading to next H2', () => {
      const md = [
        '# Design',
        '',
        '## A',
        'a-line-1',
        'a-line-2',
        '',
        '## B',
        'b-line',
      ].join('\n');
      const out = extractDesign(md);
      const a = out.sections.find((s) => s.title === 'A')!;
      expect(a.detailBody).toContain('a-line-1');
      expect(a.detailBody).toContain('a-line-2');
      expect(a.detailBody).not.toContain('b-line');
    });
  });

  it('is deterministic', () => {
    const md = [
      '# Design',
      '## Introduction',
      'intro body',
      '## A',
      'a body',
    ].join('\n');
    expect(JSON.stringify(extractDesign(md))).toBe(
      JSON.stringify(extractDesign(md))
    );
  });
});
