import { describe, expect, it } from 'bun:test';
import { lineOfSlice } from '../locate.js';

const DOC = [
  '# Requirements Document',
  '',
  '### Requirement 1: Mode Selection',
  '',
  '**User Story:** As a visitor, I want to choose.',
  '',
  '### Requirement 2: Count-Up Timing',
].join('\n');

describe('lineOfSlice', () => {
  it('finds the line a parsed item starts on', () => {
    expect(lineOfSlice(DOC, '# Requirements Document')).toBe(0);
    expect(lineOfSlice(DOC, '### Requirement 1: Mode Selection')).toBe(2);
    expect(lineOfSlice(DOC, '### Requirement 2: Count-Up Timing')).toBe(6);
  });

  it('finds a multi-line slice by its first line', () => {
    const slice = ['### Requirement 1: Mode Selection', ''].join('\n');
    expect(lineOfSlice(DOC, slice)).toBe(2);
  });

  it('falls back to the top rather than guessing', () => {
    // A document rewritten under us no longer contains the slice; landing on
    // line 0 is wrong-but-harmless, where a stale index would point anywhere.
    expect(lineOfSlice(DOC, '### Requirement 9: Gone')).toBe(0);
    expect(lineOfSlice(DOC, null)).toBe(0);
    expect(lineOfSlice('', 'anything')).toBe(0);
  });

  it('counts only newlines before the slice, not inside it', () => {
    const slice = '### Requirement 2: Count-Up Timing';
    expect(lineOfSlice(DOC, slice)).toBe(DOC.split('\n').indexOf(slice));
  });
});
