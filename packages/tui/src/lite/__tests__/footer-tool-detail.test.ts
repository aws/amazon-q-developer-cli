import { describe, it, expect } from 'bun:test';
import { extractFooterToolDetail } from '../../components/layout/lite/SubagentFooter.js';

describe('extractFooterToolDetail — lean inline-arg formatting', () => {
  it('grep: shows pattern + path in lean style', () => {
    const content = JSON.stringify({
      pattern: 'wrapAnsiLine',
      path: 'packages/tui/src',
      __tool_use_purpose: 'find usages',
    });
    const detail = extractFooterToolDetail('grep', content);
    expect(detail).toContain('wrapAnsiLine');
    expect(detail).toContain('in');
    expect(detail).toContain('packages/tui/src');
  });

  it('write tool (str_replace): shows "edit" + path', () => {
    const content = JSON.stringify({
      command: 'str_replace',
      path: 'src/lite/render.ts',
      old_str: 'foo',
      new_str: 'bar',
    });
    const detail = extractFooterToolDetail('fs_write', content);
    expect(detail).toContain('edit');
    expect(detail).toContain('src/lite/render.ts');
  });

  it('write tool (create): shows "create" + path', () => {
    const content = JSON.stringify({
      command: 'create',
      path: 'src/new-file.ts',
      file_text: 'hello',
    });
    const detail = extractFooterToolDetail('fs_write', content);
    expect(detail).toContain('create');
    expect(detail).toContain('src/new-file.ts');
  });

  it('write tool (insert): shows "insert" + path', () => {
    const content = JSON.stringify({
      command: 'insert',
      path: 'src/file.ts',
      insert_line: 10,
      new_str: 'new line',
    });
    const detail = extractFooterToolDetail('fs_write', content);
    expect(detail).toContain('insert');
    expect(detail).toContain('src/file.ts');
  });

  it('write tool (append): shows "append" + path', () => {
    const content = JSON.stringify({
      command: 'append',
      path: 'src/file.ts',
      new_str: 'appended',
    });
    const detail = extractFooterToolDetail('fs_write', content);
    expect(detail).toContain('append');
    expect(detail).toContain('src/file.ts');
  });

  it('read tool: shows the path', () => {
    const content = JSON.stringify({
      operations: [{ path: 'src/lite/render.ts', limit: 50 }],
    });
    const detail = extractFooterToolDetail('fs_read', content);
    expect(detail).toContain('src/lite/render.ts');
  });

  it('shell tool: shows the command', () => {
    const content = JSON.stringify({
      command: 'git status --short',
      __tool_use_purpose: 'check working tree',
    });
    const detail = extractFooterToolDetail('shell', content);
    expect(detail).toContain('git status --short');
  });

  it('returns null for empty content', () => {
    expect(extractFooterToolDetail('grep', '')).toBeNull();
  });

  it('returns null for unparseable content', () => {
    expect(extractFooterToolDetail('grep', 'not json')).toBeNull();
  });

  it('does not include surrounding brackets', () => {
    const content = JSON.stringify({ command: 'ls -la' });
    const detail = extractFooterToolDetail('shell', content);
    expect(detail).not.toBeNull();
    expect(detail!.startsWith('[')).toBe(false);
    expect(detail!.endsWith(']')).toBe(false);
  });
});
