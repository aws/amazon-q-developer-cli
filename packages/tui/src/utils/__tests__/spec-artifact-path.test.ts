import { describe, it, expect } from 'bun:test';
import {
  matchSpecArtifactPath,
  extractToolPath,
  isFileWriteToolName,
} from '../spec-artifact-path';

describe('matchSpecArtifactPath', () => {
  it('matches a workspace-relative requirements path', () => {
    const m = matchSpecArtifactPath(
      '.kiro/specs/login/requirements.md',
      '/work'
    );
    expect(m).not.toBeNull();
    expect(m!.featureName).toBe('login');
    expect(m!.artifact).toBe('requirements');
    expect(m!.absolutePath).toBe('/work/.kiro/specs/login/requirements.md');
  });

  it('matches a design path', () => {
    const m = matchSpecArtifactPath('.kiro/specs/x/design.md', '/work');
    expect(m).not.toBeNull();
    expect(m!.artifact).toBe('design');
  });

  it('matches a tasks path', () => {
    const m = matchSpecArtifactPath('.kiro/specs/x/tasks.md', '/work');
    expect(m).not.toBeNull();
    expect(m!.artifact).toBe('tasks');
  });

  it('matches an absolute path', () => {
    const m = matchSpecArtifactPath(
      '/Users/me/repo/.kiro/specs/feature/tasks.md',
      '/wherever'
    );
    expect(m).not.toBeNull();
    expect(m!.absolutePath).toBe(
      '/Users/me/repo/.kiro/specs/feature/tasks.md'
    );
    expect(m!.featureName).toBe('feature');
  });

  it('rejects nested feature directories (path separator in feature name)', () => {
    const m = matchSpecArtifactPath(
      '.kiro/specs/parent/child/requirements.md',
      '/work'
    );
    expect(m).toBeNull();
  });

  it('rejects files outside .kiro/specs', () => {
    expect(
      matchSpecArtifactPath('docs/requirements.md', '/work')
    ).toBeNull();
    expect(
      matchSpecArtifactPath('.kiro/other/requirements.md', '/work')
    ).toBeNull();
  });

  it('rejects unsupported artifact names', () => {
    expect(
      matchSpecArtifactPath('.kiro/specs/x/notes.md', '/work')
    ).toBeNull();
    expect(
      matchSpecArtifactPath('.kiro/specs/x/bugfix.md', '/work')
    ).toBeNull();
  });

  it('returns null for empty / non-string input', () => {
    expect(matchSpecArtifactPath('', '/work')).toBeNull();
    expect(matchSpecArtifactPath(null as unknown as string, '/work')).toBeNull();
  });
});

describe('extractToolPath', () => {
  it('returns string path from args', () => {
    expect(extractToolPath({ path: 'foo.md' })).toBe('foo.md');
  });

  it('returns null when path is missing', () => {
    expect(extractToolPath({})).toBeNull();
    expect(extractToolPath(undefined)).toBeNull();
  });

  it('returns null when path is non-string', () => {
    expect(extractToolPath({ path: 42 } as any)).toBeNull();
  });
});

describe('isFileWriteToolName', () => {
  it('accepts fs_write and Write', () => {
    expect(isFileWriteToolName('fs_write')).toBe(true);
    expect(isFileWriteToolName('Write')).toBe(true);
  });

  it('rejects unrelated tool names', () => {
    expect(isFileWriteToolName('fs_read')).toBe(false);
    expect(isFileWriteToolName('write')).toBe(false); // lowercase doesn't match
    expect(isFileWriteToolName('execute_bash')).toBe(false);
  });
});
