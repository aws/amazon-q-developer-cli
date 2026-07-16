import { describe, expect, it } from 'bun:test';

import type {
  SourceProviderResourcePage,
  SourceProviderResourcesRequest,
} from '@kiro/acp-type-covenant';

import { drainConnectedProviderRepos } from '../cloud-repo-drain';

function repo(name: string) {
  return { providerType: 'GITHUB', name };
}

/** Build a lister that returns fixed pages keyed by `providerType:cursor`. */
function lister(pages: Record<string, SourceProviderResourcePage>) {
  const calls: SourceProviderResourcesRequest[] = [];
  const fn = async (req: SourceProviderResourcesRequest) => {
    calls.push(req);
    return pages[`${req.providerType}:${req.cursor ?? ''}`];
  };
  return { fn, calls };
}

describe('drainConnectedProviderRepos', () => {
  it('pages one provider to exhaustion via nextCursor', async () => {
    const { fn } = lister({
      'GITHUB:': { resources: [repo('a/1'), repo('a/2')], nextCursor: 'p2' },
      'GITHUB:p2': { resources: [repo('a/3')] },
    });
    const { resources, partial } = await drainConnectedProviderRepos(fn, [
      'GITHUB',
    ]);
    expect(resources.map((r) => r.name)).toEqual(['a/1', 'a/2', 'a/3']);
    expect(partial).toBe(false);
  });

  it('aggregates across providers and dedupes by name', async () => {
    const { fn } = lister({
      'GITHUB:': { resources: [repo('x'), repo('shared')] },
      'GITLAB:': { resources: [repo('shared'), repo('y')] },
    });
    const { resources } = await drainConnectedProviderRepos(fn, [
      'GITHUB',
      'GITLAB',
    ]);
    expect(resources.map((r) => r.name)).toEqual(['x', 'shared', 'y']);
  });

  it('flags partial and stops when a provider exceeds the page cap', async () => {
    // Every page returns a nextCursor, so the drain never naturally ends —
    // the maxPages cap must cut it off and report the count as a lower bound.
    const calls: SourceProviderResourcesRequest[] = [];
    const looping = async (req: SourceProviderResourcesRequest) => {
      calls.push(req);
      return { resources: [repo(`r${calls.length}`)], nextCursor: 'more' };
    };
    const { resources, partial } = await drainConnectedProviderRepos(
      looping,
      ['GITHUB'],
      3
    );
    expect(partial).toBe(true);
    expect(resources).toHaveLength(3);
  });

  it('stops a provider cleanly when a page is undefined', async () => {
    const { fn } = lister({
      'GITHUB:': { resources: [repo('a')], nextCursor: 'p2' },
      // 'GITHUB:p2' intentionally absent → lister returns undefined.
    });
    const { resources } = await drainConnectedProviderRepos(fn, ['GITHUB']);
    expect(resources.map((r) => r.name)).toEqual(['a']);
  });
});
