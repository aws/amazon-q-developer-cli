import { describe, it, expect } from 'bun:test';
import { parseArtifact, bytesToUtf8, type ArtifactKind } from '../index';

/**
 * Property-style robustness tests for the parser.
 *
 * We don't have `fast-check` available so we use a seeded linear
 * congruential generator to produce reproducible "arbitrary" inputs,
 * then assert the documented invariants:
 *
 *   1. Same input produces the same output (deterministic).
 *   2. The parser never throws, regardless of input bytes.
 *   3. Empty / whitespace-only input produces empty extracted sets.
 *   4. The output structure is well-formed for every artifact kind.
 *   5. UTF-8 decode of arbitrary bytes never throws.
 */

function lcg(seed: number) {
  let s = seed >>> 0;
  return () => {
    // Numerical Recipes constants
    s = (s * 1664525 + 1013904223) >>> 0;
    return s;
  };
}

function randomBytes(seed: number, len: number): Buffer {
  const rng = lcg(seed);
  const buf = Buffer.alloc(len);
  for (let i = 0; i < len; i++) {
    buf[i] = rng() & 0xff;
  }
  return buf;
}

function randomMixedMarkdown(seed: number, len: number): string {
  const rng = lcg(seed);
  const tokens = [
    '### Requirement 1: ',
    '### Requirement 2: ',
    '## Section ',
    '## Introduction\n',
    '## Architecture Overview\n',
    '**User Story:** ',
    '- [ ] 1. ',
    '- [x] 1. ',
    '  - [ ] 1.1. ',
    '\n',
    'lorem ',
    'ipsum ',
    'dolor ',
    'sit ',
    'amet ',
    '\u0000', // null byte
    '\uFFFD', // replacement char
  ];
  const out: string[] = [];
  let total = 0;
  while (total < len) {
    const t = tokens[rng() % tokens.length]!;
    out.push(t);
    total += t.length;
  }
  return out.join('');
}

const KINDS: ArtifactKind[] = ['requirements', 'design', 'tasks'];

describe('parser robustness', () => {
  it('never throws on empty input for any kind', () => {
    for (const kind of KINDS) {
      expect(() => parseArtifact(kind, '')).not.toThrow();
    }
  });

  it('never throws on whitespace-only input for any kind', () => {
    for (const kind of KINDS) {
      expect(() => parseArtifact(kind, '   \n\t\n  ')).not.toThrow();
    }
  });

  it('produces empty sets for empty input', () => {
    const r = parseArtifact('requirements', '');
    if (r.kind !== 'requirements') throw new Error('kind mismatch');
    expect(r.items).toEqual([]);

    const d = parseArtifact('design', '');
    if (d.kind !== 'design') throw new Error('kind mismatch');
    expect(d.overview).toBe('');
    expect(d.sections).toEqual([]);

    const t = parseArtifact('tasks', '');
    if (t.kind !== 'tasks') throw new Error('kind mismatch');
    expect(t.items).toEqual([]);
  });

  it('is deterministic across seeded random inputs', () => {
    for (let seed = 1; seed < 30; seed++) {
      const input = randomMixedMarkdown(seed, 1024);
      for (const kind of KINDS) {
        const a = parseArtifact(kind, input);
        const b = parseArtifact(kind, input);
        expect(JSON.stringify(a)).toBe(JSON.stringify(b));
      }
    }
  });

  it('never throws on seeded random byte sequences (utf8 substitution)', () => {
    for (let seed = 1; seed < 20; seed++) {
      const bytes = randomBytes(seed, 4096);
      const decoded = bytesToUtf8(bytes);
      // Decode itself is allowed to substitute U+FFFD; just ensure no throw.
      expect(typeof decoded).toBe('string');
      for (const kind of KINDS) {
        expect(() => parseArtifact(kind, decoded)).not.toThrow();
      }
    }
  });

  it('output for tasks always has well-formed shape', () => {
    for (let seed = 1; seed < 15; seed++) {
      const input = randomMixedMarkdown(seed, 2048);
      const out = parseArtifact('tasks', input);
      if (out.kind !== 'tasks') throw new Error('kind mismatch');
      for (const item of out.items) {
        expect(typeof item.number).toBe('string');
        expect(typeof item.title).toBe('string');
        expect(typeof item.checked).toBe('boolean');
        expect(Array.isArray(item.subTasks)).toBe(true);
        expect(typeof item.detailBody).toBe('string');
        for (const s of item.subTasks) {
          expect(typeof s.title).toBe('string');
          expect(typeof s.checked).toBe('boolean');
          expect(typeof s.depth).toBe('number');
        }
      }
    }
  });

  it('output for requirements always has well-formed shape', () => {
    for (let seed = 1; seed < 15; seed++) {
      const input = randomMixedMarkdown(seed, 2048);
      const out = parseArtifact('requirements', input);
      if (out.kind !== 'requirements') throw new Error('kind mismatch');
      for (const item of out.items) {
        expect(typeof item.number).toBe('number');
        expect(typeof item.title).toBe('string');
        expect(
          item.userStory === null || typeof item.userStory === 'string'
        ).toBe(true);
        expect(typeof item.detailBody).toBe('string');
      }
    }
  });

  it('output for design has well-formed shape', () => {
    for (let seed = 1; seed < 15; seed++) {
      const input = randomMixedMarkdown(seed, 2048);
      const out = parseArtifact('design', input);
      if (out.kind !== 'design') throw new Error('kind mismatch');
      expect(typeof out.overview).toBe('string');
      expect(typeof out.overviewTruncated).toBe('boolean');
      for (const s of out.sections) {
        expect(typeof s.title).toBe('string');
        expect(typeof s.detailBody).toBe('string');
      }
    }
  });

  it('round-trip: render → parse preserves count and identifiers (requirements)', () => {
    // Build a spec with N requirements, render it, parse it back.
    for (let n = 1; n <= 5; n++) {
      const lines: string[] = ['# Requirements', ''];
      for (let i = 1; i <= n; i++) {
        lines.push(`### Requirement ${i}: Title ${i}`);
        lines.push(`**User Story:** As u${i}`);
        lines.push('');
      }
      const md = lines.join('\n');
      const out = parseArtifact('requirements', md);
      if (out.kind !== 'requirements') throw new Error('kind mismatch');
      expect(out.items).toHaveLength(n);
      expect(out.items.map((r) => r.number)).toEqual(
        Array.from({ length: n }, (_, i) => i + 1)
      );
      expect(out.items.map((r) => r.title)).toEqual(
        Array.from({ length: n }, (_, i) => `Title ${i + 1}`)
      );
    }
  });
});
