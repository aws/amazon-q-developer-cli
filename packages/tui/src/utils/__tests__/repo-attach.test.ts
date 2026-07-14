import { describe, it, expect } from 'bun:test';
import {
  normalizeRepoArg,
  formatCloneRepoInstruction,
  formatCloneReposInstruction,
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
