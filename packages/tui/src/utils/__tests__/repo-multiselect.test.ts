import { describe, it, expect } from 'bun:test';
import {
  filterRepoResources,
  toggleRepoSelection,
  formatRepoLastUsed,
  dedupeRepoResources,
} from '../repo-multiselect';
import { UNICODE_GLYPHS } from '../glyphs';
import type { SourceProviderResource } from '@kiro/acp-type-covenant';

const repos: SourceProviderResource[] = [
  { providerType: 'GITHUB', name: 'kiro/banana-service' },
  { providerType: 'GITHUB', name: 'kiro/frontend-app' },
  { providerType: 'GITHUB', name: 'acme/shared-libs' },
];

describe('filterRepoResources', () => {
  it('returns all repos for an empty / whitespace query', () => {
    expect(filterRepoResources(repos, '')).toHaveLength(3);
    expect(filterRepoResources(repos, '   ')).toHaveLength(3);
  });

  it('filters by case-insensitive substring on name', () => {
    expect(filterRepoResources(repos, 'kiro').map((r) => r.name)).toEqual([
      'kiro/banana-service',
      'kiro/frontend-app',
    ]);
    expect(filterRepoResources(repos, 'SHARED').map((r) => r.name)).toEqual([
      'acme/shared-libs',
    ]);
  });

  it('returns empty when nothing matches', () => {
    expect(filterRepoResources(repos, 'zzz')).toHaveLength(0);
  });
});

describe('toggleRepoSelection', () => {
  it('appends a newly selected repo (order = selection order)', () => {
    expect(toggleRepoSelection([], 'a')).toEqual(['a']);
    expect(toggleRepoSelection(['a'], 'b')).toEqual(['a', 'b']);
  });

  it('removes an already-selected repo', () => {
    expect(toggleRepoSelection(['a', 'b', 'c'], 'b')).toEqual(['a', 'c']);
  });

  it('does not mutate the input array', () => {
    const input = ['a'];
    const out = toggleRepoSelection(input, 'b');
    expect(input).toEqual(['a']);
    expect(out).toEqual(['a', 'b']);
  });
});

describe('formatRepoLastUsed', () => {
  it('returns the em-dash glyph when updatedAt is absent or unparseable', () => {
    expect(formatRepoLastUsed(undefined)).toBe(UNICODE_GLYPHS.emDash);
    expect(formatRepoLastUsed('not-a-date')).toBe(UNICODE_GLYPHS.emDash);
  });

  it('renders compact relative buckets', () => {
    const ago = (secs: number) =>
      new Date(Date.now() - secs * 1000).toISOString();
    expect(formatRepoLastUsed(ago(10))).toBe('just now');
    expect(formatRepoLastUsed(ago(5 * 60))).toBe('5m ago');
    expect(formatRepoLastUsed(ago(3 * 3600))).toBe('3h ago');
    expect(formatRepoLastUsed(ago(2 * 86400))).toBe('2d ago');
    expect(formatRepoLastUsed(ago(14 * 86400))).toBe('2w ago');
  });
});

describe('dedupeRepoResources', () => {
  it('returns an empty list unchanged', () => {
    expect(dedupeRepoResources([])).toEqual([]);
  });

  it('keeps the first row for each repeated name and preserves order', () => {
    const dupes: SourceProviderResource[] = [
      { providerType: 'GITHUB', name: 'kiro/banana-service' },
      { providerType: 'GITLAB', name: 'kiro/banana-service' },
      { providerType: 'GITHUB', name: 'acme/shared-libs' },
      { providerType: 'GITHUB', name: 'kiro/banana-service' },
    ];
    const out = dedupeRepoResources(dupes);
    expect(out.map((r) => r.name)).toEqual([
      'kiro/banana-service',
      'acme/shared-libs',
    ]);
    // First sighting wins: the GITHUB row is kept, later GITLAB duplicate dropped.
    expect(out[0]!.providerType).toBe('GITHUB');
  });

  it('leaves an already-unique list untouched', () => {
    expect(dedupeRepoResources(repos).map((r) => r.name)).toEqual(
      repos.map((r) => r.name)
    );
  });
});
