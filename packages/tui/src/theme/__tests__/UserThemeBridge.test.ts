import { describe, it, expect, mock } from 'bun:test';
import { buildAutoPreview, extractThemeDiffHex } from '../UserThemeBridge';
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

  it('contains "This is how your prompt will look" with kiroDark', () => {
    const result = buildAutoPreview(kiroDark.colors);
    expect(result).toContain(PROMPT_PREVIEW);
  });

  it('contains "This is how the response will look" with kiroDark', () => {
    const result = buildAutoPreview(kiroDark.colors);
    expect(result).toContain(RESPONSE_PREVIEW);
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

describe('extractThemeDiffHex', () => {
  it('returns correct structure with all string values', () => {
    const mockGetColor = mock((_path: string) => ({ hex: '#aabbcc' }));
    const result = extractThemeDiffHex(mockGetColor);

    expect(typeof result.added.background).toBe('string');
    expect(typeof result.added.bar).toBe('string');
    expect(typeof result.added.highlight).toBe('string');
    expect(typeof result.removed.background).toBe('string');
    expect(typeof result.removed.bar).toBe('string');
    expect(typeof result.removed.highlight).toBe('string');
  });

  it('returns hex values from getColor calls', () => {
    const mockGetColor = mock((_path: string) => ({ hex: '#aabbcc' }));
    const result = extractThemeDiffHex(mockGetColor);
    expect(result.added.background).toBe('#aabbcc');
    expect(result.removed.bar).toBe('#aabbcc');
  });

  it('passes correct paths to getColor', () => {
    const mockGetColor = mock((path: string) => {
      const hexMap: Record<string, string> = {
        'diff.added.background': '#111111',
        'diff.added.bar': '#222222',
        'diff.added.highlight': '#333333',
        'diff.removed.background': '#444444',
        'diff.removed.bar': '#555555',
        'diff.removed.highlight': '#666666',
      };
      return { hex: hexMap[path] ?? '#000000' };
    });
    const result = extractThemeDiffHex(mockGetColor);
    expect(result.added.background).toBe('#111111');
    expect(result.added.bar).toBe('#222222');
    expect(result.added.highlight).toBe('#333333');
    expect(result.removed.background).toBe('#444444');
    expect(result.removed.bar).toBe('#555555');
    expect(result.removed.highlight).toBe('#666666');
  });
});
