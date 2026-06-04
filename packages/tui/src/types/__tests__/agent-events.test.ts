import { describe, it, expect } from 'bun:test';
import { resolveToolId, kindToToolId } from '../agent-events';

describe('resolveToolId', () => {
  it('resolves write tools', () => {
    expect(resolveToolId('fs_write')).toBe('write');
    expect(resolveToolId('write')).toBe('write');
    expect(resolveToolId('str_replace')).toBe('write');
    expect(resolveToolId('fs_append')).toBe('write');
    expect(resolveToolId('delete_file')).toBe('write');
  });

  it('resolves read tools', () => {
    expect(resolveToolId('fs_read')).toBe('read');
    expect(resolveToolId('read')).toBe('read');
    expect(resolveToolId('read_file')).toBe('read');
    expect(resolveToolId('read_files')).toBe('read');
    expect(resolveToolId('list_directory')).toBe('read');
  });

  it('resolves shell tools', () => {
    expect(resolveToolId('execute_bash')).toBe('shell');
    expect(resolveToolId('shell')).toBe('shell');
    expect(resolveToolId('control_bash_process')).toBe('shell');
    expect(resolveToolId('control_pwsh_process')).toBe('shell');
  });

  it('resolves web_search tools', () => {
    expect(resolveToolId('web_search')).toBe('web_search');
  });

  it('resolves web_fetch tools', () => {
    expect(resolveToolId('web_fetch')).toBe('web_fetch');
  });

  it('resolves grep tools', () => {
    expect(resolveToolId('grep')).toBe('grep');
    expect(resolveToolId('grep_search')).toBe('grep');
  });

  it('resolves glob tools', () => {
    expect(resolveToolId('glob')).toBe('glob');
    expect(resolveToolId('file_search')).toBe('glob');
  });

  it('resolves ls tools', () => {
    expect(resolveToolId('ls')).toBe('ls');
  });

  it('resolves code tools', () => {
    expect(resolveToolId('code')).toBe('code');
  });

  it('resolves image_read tools', () => {
    expect(resolveToolId('imageRead')).toBe('image_read');
  });

  it('resolves task tools', () => {
    expect(resolveToolId('task')).toBe('task');
    expect(resolveToolId('todo_list')).toBe('task');
  });

  it('returns undefined for unknown tools', () => {
    expect(resolveToolId('custom_mcp_tool')).toBeUndefined();
  });
});

describe('kindToToolId', () => {
  it('maps the kinds that routing keys on', () => {
    expect(kindToToolId('read')).toBe('read');
    expect(kindToToolId('edit')).toBe('write');
    expect(kindToToolId('execute')).toBe('shell');
    expect(kindToToolId('search')).toBe('grep');
  });

  it('returns undefined for other kinds and undefined', () => {
    expect(kindToToolId(undefined)).toBeUndefined();
  });
});
