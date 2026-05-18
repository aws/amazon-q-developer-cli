import { describe, it, expect } from 'bun:test';
import {
  matchSpecArtifactPath,
  extractToolPath,
  isFileWriteToolName,
  isWriteOperation,
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
  it('accepts the V1 alias and KAS native names', () => {
    expect(isFileWriteToolName('fs_write')).toBe(true);
    expect(isFileWriteToolName('Write')).toBe(true);
  });

  it('accepts the multiplex KAS spec workflow names', () => {
    // KAS spec workflow has been observed surfacing writes as `create`,
    // `Edit`, or `fs_edit` rather than the single-purpose write tools.
    expect(isFileWriteToolName('create')).toBe(true);
    expect(isFileWriteToolName('Edit')).toBe(true);
    expect(isFileWriteToolName('fs_edit')).toBe(true);
  });

  it('rejects unrelated tool names', () => {
    expect(isFileWriteToolName('fs_read')).toBe(false);
    expect(isFileWriteToolName('write')).toBe(false); // case-sensitive
    expect(isFileWriteToolName('execute_bash')).toBe(false);
    expect(isFileWriteToolName('Read')).toBe(false);
  });
});

describe('isWriteOperation', () => {
  it('returns true when args has no command field', () => {
    // Single-purpose tools (fs_write / Write / create) don't carry a
    // `command` arg; we should always treat them as writes.
    expect(isWriteOperation({})).toBe(true);
    expect(isWriteOperation({ path: 'foo.md' })).toBe(true);
    expect(isWriteOperation(undefined)).toBe(true);
  });

  it('returns true for recognised write commands', () => {
    expect(isWriteOperation({ command: 'create' })).toBe(true);
    expect(isWriteOperation({ command: 'update' })).toBe(true);
    expect(isWriteOperation({ command: 'replace' })).toBe(true);
    expect(isWriteOperation({ command: 'write' })).toBe(true);
    expect(isWriteOperation({ command: 'overwrite' })).toBe(true);
  });

  it('returns false for non-write commands on multiplex tools', () => {
    expect(isWriteOperation({ command: 'delete' })).toBe(false);
    expect(isWriteOperation({ command: 'move' })).toBe(false);
    expect(isWriteOperation({ command: 'rename' })).toBe(false);
  });

  it('returns false for non-string command values', () => {
    expect(isWriteOperation({ command: 42 } as any)).toBe(false);
    expect(isWriteOperation({ command: true } as any)).toBe(false);
  });
});
