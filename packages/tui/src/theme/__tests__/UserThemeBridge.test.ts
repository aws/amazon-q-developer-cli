import { describe, it, expect } from 'bun:test';
import { buildAutoPreview, extractThemeDiffColors } from '../UserThemeBridge';
import { kiroDark } from '../kiroDark';
import { kiroLight } from '../kiroLight';
import { kiroSafe } from '../kiroSafe';
import {
  PROMPT_PREVIEW,
  RESPONSE_PREVIEW,
  DIFF_ADDED_PREVIEW,
  DIFF_REMOVED_PREVIEW,
} from '../user-theme';

describe('buildAutoPreview', () => {
  it('returns non-empty string with kiroDark colors', () => {
    const result = buildAutoPreview(kiroDark.colors);
    expect(result.length).toBeGreaterThan(0);
  });

  it('contains "This is the user input" with kiroDark', () => {
    const result = buildAutoPreview(kiroDark.colors);
    expect(result).toContain(PROMPT_PREVIEW);
  });

  it('contains "This is the system response" with kiroDark', () => {
    const result = buildAutoPreview(kiroDark.colors);
    expect(result).toContain(RESPONSE_PREVIEW);
  });

  it('uses brand colour for the ▌ bar (not surface or primary)', () => {
    // After the spec UX update, both prompt and response rows share the
    // same brand-coloured bar, matching `<StatusBar>` in the live
    // conversation. kiroDark's brand is #C19AFF → 193;154;255 in RGB.
    const result = buildAutoPreview(kiroDark.colors);
    expect(result).toContain('48;2;193;154;255');
  });

  it('contains diff added and removed preview lines with kiroDark', () => {
    const result = buildAutoPreview(kiroDark.colors);
    expect(result).toContain(DIFF_ADDED_PREVIEW);
    expect(result).toContain(DIFF_REMOVED_PREVIEW);
  });

  it('returns non-empty string with kiroLight colors', () => {
    const result = buildAutoPreview(kiroLight.colors);
    expect(result.length).toBeGreaterThan(0);
  });

  it('contains diff added and removed preview lines with kiroLight', () => {
    const result = buildAutoPreview(kiroLight.colors);
    expect(result).toContain(DIFF_ADDED_PREVIEW);
    expect(result).toContain(DIFF_REMOVED_PREVIEW);
  });

  it('returns non-empty string with kiroSafe colors (no truecolor values)', () => {
    const result = buildAutoPreview(kiroSafe.colors);
    expect(result.length).toBeGreaterThan(0);
  });

  it('contains diff added and removed preview lines with kiroSafe', () => {
    const result = buildAutoPreview(kiroSafe.colors);
    expect(result).toContain(DIFF_ADDED_PREVIEW);
    expect(result).toContain(DIFF_REMOVED_PREVIEW);
  });
});

describe('extractThemeDiffColors', () => {
  it('returns TerminalColor objects from theme colors', () => {
    const result = extractThemeDiffColors(kiroDark.colors);
    expect(result.added.background).toEqual(
      kiroDark.colors.diff.added.background
    );
    expect(result.added.bar).toEqual(kiroDark.colors.diff.added.bar);
    expect(result.added.highlight).toEqual(
      kiroDark.colors.diff.added.highlight
    );
    expect(result.removed.background).toEqual(
      kiroDark.colors.diff.removed.background
    );
    expect(result.removed.bar).toEqual(kiroDark.colors.diff.removed.bar);
    expect(result.removed.highlight).toEqual(
      kiroDark.colors.diff.removed.highlight
    );
  });

  it('preserves color256 values', () => {
    const result = extractThemeDiffColors(kiroDark.colors);
    // kiroDark diff colors have color256 set
    expect(result.added.bar.color256).toBeDefined();
    expect(result.removed.bar.color256).toBeDefined();
  });

  it('works with kiroSafe named colors', () => {
    const result = extractThemeDiffColors(kiroSafe.colors);
    expect(result.added.bar.named).toBe('green');
    expect(result.removed.bar.named).toBe('red');
  });
});
