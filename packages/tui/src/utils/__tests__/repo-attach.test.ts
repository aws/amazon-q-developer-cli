import { describe, it, expect } from 'bun:test';
import {
  normalizeRepoArg,
  formatCloneRepoInstruction,
  formatCloneReposInstruction,
  resolveSourceProviderConnection,
  droppedReposFromWarnings,
  formatRepoChangeInstruction,
} from '../repo-attach';

describe('normalizeRepoArg', () => {
  it('trims and returns a non-empty repo arg', () => {
    expect(normalizeRepoArg('  owner/repo  ')).toBe('owner/repo');
    expect(normalizeRepoArg('gitlab:group/project')).toBe(
      'gitlab:group/project'
    );
    expect(normalizeRepoArg('MyPackage')).toBe('MyPackage');
  });

  it('returns undefined for empty / whitespace / missing args', () => {
    expect(normalizeRepoArg('')).toBeUndefined();
    expect(normalizeRepoArg('   ')).toBeUndefined();
    expect(normalizeRepoArg(undefined)).toBeUndefined();
  });
});

describe('formatCloneRepoInstruction', () => {
  it('builds the natural-language clone prompt for the repo', () => {
    expect(formatCloneRepoInstruction('owner/repo')).toBe(
      'Clone the repository owner/repo into the workspace.'
    );
  });

  it('passes the bind value through verbatim', () => {
    expect(formatCloneRepoInstruction('gitlab:group/project')).toContain(
      'gitlab:group/project'
    );
  });
});

describe('formatCloneReposInstruction', () => {
  it('returns empty string when nothing usable is selected', () => {
    expect(formatCloneReposInstruction([])).toBe('');
    expect(formatCloneReposInstruction(['', '  '])).toBe('');
  });

  it('delegates to the single-repo form for exactly one usable repo', () => {
    expect(formatCloneReposInstruction(['owner/repo'])).toBe(
      'Clone the repository owner/repo into the workspace.'
    );
    // blanks dropped -> single remaining repo uses the single form
    expect(formatCloneReposInstruction(['  ', 'acme/api'])).toBe(
      'Clone the repository acme/api into the workspace.'
    );
  });

  it('lists multiple repos in a single instruction, in order', () => {
    expect(
      formatCloneReposInstruction(['acme/web', 'acme/api', 'kiro/shared-libs'])
    ).toBe(
      'Clone the following repositories into the workspace: acme/web, acme/api, kiro/shared-libs.'
    );
  });

  it('trims each entry', () => {
    expect(formatCloneReposInstruction([' acme/web ', ' acme/api '])).toBe(
      'Clone the following repositories into the workspace: acme/web, acme/api.'
    );
  });
});

describe('resolveSourceProviderConnection', () => {
  it('reports not connected with no setupUrl for an undefined listing', () => {
    expect(resolveSourceProviderConnection(undefined)).toEqual({
      connected: false,
      connectedProviders: [],
    });
  });

  it('lists every connected provider display name', () => {
    const list = {
      providers: [
        {
          providerType: 'GITHUB',
          displayName: 'GitHub',
          connectionStatus: 'connected' as const,
        },
        {
          providerType: 'GITFARM',
          displayName: 'GitFarm',
          connectionStatus: 'connected' as const,
        },
        {
          providerType: 'GITLAB',
          displayName: 'GitLab',
          connectionStatus: 'not_connected' as const,
        },
      ],
    };
    expect(resolveSourceProviderConnection(list)).toEqual({
      connected: true,
      connectedProviders: ['GitHub', 'GitFarm'],
    });
  });

  it('surfaces the setupUrl from a not_connected provider', () => {
    const list = {
      providers: [
        {
          providerType: 'GITHUB',
          displayName: 'GitHub',
          connectionStatus: 'not_connected' as const,
          setupUrl: 'https://kiro.dev/connect',
        },
      ],
    };
    expect(resolveSourceProviderConnection(list)).toEqual({
      connected: false,
      connectedProviders: [],
      setupUrl: 'https://kiro.dev/connect',
    });
  });
});

describe('droppedReposFromWarnings', () => {
  it('extracts quoted repo names from KAS bind warnings', () => {
    const got = droppedReposFromWarnings([
      'repository "acme/ghost" was not found among your connected source providers',
      'repository "typo-repo" is not a valid repository reference',
    ]);
    expect(got).toEqual(new Set(['acme/ghost', 'typo-repo']));
  });

  it('ignores warnings without a quoted repo name', () => {
    expect(droppedReposFromWarnings(['some unrelated warning'])).toEqual(
      new Set()
    );
  });
});

describe('formatRepoChangeInstruction', () => {
  it('returns an empty string when nothing was added or removed', () => {
    expect(formatRepoChangeInstruction([], [])).toBe('');
    expect(formatRepoChangeInstruction(['  '], ['  '])).toBe('');
  });

  it('emits only a clone instruction when repos are added', () => {
    expect(formatRepoChangeInstruction(['owner/repo'], [])).toBe(
      formatCloneReposInstruction(['owner/repo'])
    );
  });

  it('emits only a remove instruction for a single removal', () => {
    expect(formatRepoChangeInstruction([], ['owner/gone'])).toBe(
      'Remove the repository owner/gone from the workspace (delete its cloned directory).'
    );
  });

  it('lists multiple removals in one instruction', () => {
    expect(formatRepoChangeInstruction([], ['a/one', 'b/two'])).toBe(
      'Remove the following repositories from the workspace (delete their cloned directories): a/one, b/two.'
    );
  });

  it('joins clone and remove instructions with a single space', () => {
    const both = formatRepoChangeInstruction(['a/add'], ['b/gone']);
    expect(both).toBe(
      `${formatCloneReposInstruction(['a/add'])} Remove the repository b/gone from the workspace (delete its cloned directory).`
    );
  });

  it('trims and drops blank removals', () => {
    expect(formatRepoChangeInstruction([], ['  a/one  ', ''])).toBe(
      'Remove the repository a/one from the workspace (delete its cloned directory).'
    );
  });
});
