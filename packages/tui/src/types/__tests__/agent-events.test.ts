import { describe, it, expect } from 'bun:test';
import {
  resolveToolId,
  kindToToolId,
  isParentSubagentTool,
  deriveToolDiff,
} from '../agent-events';
import type { ToolCallEvent } from '../agent-events';

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

  it('resolves knowledge tools', () => {
    expect(resolveToolId('knowledge')).toBe('knowledge');
    expect(resolveToolId('Knowledge Search')).toBe('knowledge');
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

describe('isParentSubagentTool', () => {
  it('recognizes the plain subagent parent', () => {
    expect(isParentSubagentTool('subagent')).toBe(true);
  });

  it('recognizes the pipeline parent renamed to orchestrate_subagent', () => {
    // Regression: a v2 agent_crew pipeline emits the parent as
    // 'orchestrate_subagent' (kiroMeta.pipeline rename). Lite must still
    // recognize it as a subagent parent, else grouping/hiding breaks and
    // per-stage rows flood scrollback.
    expect(isParentSubagentTool('orchestrate_subagent')).toBe(true);
    expect(isParentSubagentTool('invoke_sub_agent')).toBe(true);
    expect(isParentSubagentTool('agent_crew')).toBe(true);
  });

  it('does not treat response/management tools as parents', () => {
    expect(isParentSubagentTool('subagent_response')).toBe(false);
    expect(isParentSubagentTool('session_management')).toBe(false);
    expect(isParentSubagentTool('fs_read')).toBe(false);
    expect(isParentSubagentTool(undefined)).toBe(false);
    expect(isParentSubagentTool(null)).toBe(false);
  });
});

describe('deriveToolDiff (transport-agnostic diff rendering for cloud sessions)', () => {
  // A cloud session's edited file lives on the sandbox, not the
  // local disk. Diff previews must be built purely from the ACP payload / tool
  // input and must NEVER read a local file — otherwise a cloud diff would show
  // wrong/empty content (or throw) for a path that doesn't exist locally.

  it('derives a diff from the ACP toolContent payload even for a sandbox-only path (no local file read)', () => {
    const event = {
      type: 'tool_call',
      kind: 'write',
      // A path that does not exist on the local machine — proves the diff comes
      // from the wire payload, not a filesystem read.
      toolContent: [
        {
          type: 'diff',
          path: '/nonexistent/sandbox/only/sandbox-file.ts',
          newText: 'export const x = 2;\n',
          oldText: 'export const x = 1;\n',
        },
      ],
    } as unknown as ToolCallEvent;

    expect(deriveToolDiff(event)).toEqual({
      path: '/nonexistent/sandbox/only/sandbox-file.ts',
      newText: 'export const x = 2;\n',
      oldText: 'export const x = 1;\n',
    });
  });

  it('derives a diff from the edit tool input (args) when no explicit toolContent is present', () => {
    const event = {
      type: 'tool_call',
      kind: 'edit',
      args: {
        path: '/nonexistent/sandbox/only/app.ts',
        oldStr: 'a',
        newStr: 'b',
      },
    } as unknown as ToolCallEvent;

    expect(deriveToolDiff(event)).toEqual({
      path: '/nonexistent/sandbox/only/app.ts',
      newText: 'b',
      oldText: 'a',
    });
  });

  it('returns undefined when no diff can be derived (no payload, non-edit kind)', () => {
    const event = {
      type: 'tool_call',
      kind: 'shell',
      args: { command: 'ls' },
    } as unknown as ToolCallEvent;

    expect(deriveToolDiff(event)).toBeUndefined();
  });
});
