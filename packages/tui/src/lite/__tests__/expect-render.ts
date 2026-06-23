import { expect } from 'vitest';
import stripAnsi from 'strip-ansi';

/**
 * Shared assertion harness for the lite render suites. Every suite repeats the
 * same "strip ANSI, then assert contains/absent/matches on the stripped text
 * and rawContains on the un-stripped text" loop; this collapses it to one call.
 *
 * `raw` is the un-stripped renderer output. Substring/regex assertions
 * (`contains`/`absent`/`matches`/`notMatches`) run on the ANSI-stripped form;
 * `rawContains`/`rawAbsent` run on the raw output (for SGR-escape checks).
 */
export interface RenderSpec {
  contains?: readonly string[];
  absent?: readonly string[];
  matches?: readonly RegExp[];
  notMatches?: readonly RegExp[];
  rawContains?: readonly string[];
  rawAbsent?: readonly string[];
}

export function expectRender(raw: string, spec: RenderSpec): void {
  const stripped = stripAnsi(raw);
  for (const s of spec.contains ?? []) expect(stripped).toContain(s);
  for (const s of spec.absent ?? []) expect(stripped).not.toContain(s);
  for (const re of spec.matches ?? []) expect(stripped).toMatch(re);
  for (const re of spec.notMatches ?? []) expect(stripped).not.toMatch(re);
  for (const s of spec.rawContains ?? []) expect(raw).toContain(s);
  for (const s of spec.rawAbsent ?? []) expect(raw).not.toContain(s);
}

/**
 * Slice a stripped block between two marker substrings (e.g. the "full output:"
 * → "response summary:" window). `from`/`to` are located via indexOf; an absent
 * marker yields a slice starting/ending at the string bound.
 */
export function section(stripped: string, from: string, to?: string): string {
  const start = stripped.indexOf(from);
  const end = to ? stripped.indexOf(to) : stripped.length;
  return stripped.slice(start, end >= 0 ? end : stripped.length);
}
