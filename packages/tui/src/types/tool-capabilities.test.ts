import { describe, expect, it } from 'bun:test';
import {
  NON_SCROLLBACK_TOOL_IDS,
  TOOL_CAPABILITIES,
  TOOL_RENDERER_APPROVAL,
  TOOL_RENDERER_VERBOSITY,
  isTrivialTool,
  resolveScrollbackToolRenderer,
  resolveToolDisplayName,
  resolveToolId,
  toolApprovalDetail,
  toolApprovalPresentation,
  toolDiffPolicy,
  toolVerbosityPolicy,
  type ToolDiffPolicy,
} from './tool-capabilities.js';
import { TOOL_LABELS } from './tool-status.js';

describe('tool capability registry', () => {
  it('classifies every built-in exactly once', () => {
    const registered = Object.values(TOOL_CAPABILITIES).flatMap((capability) =>
      'builtinId' in capability ? [capability.builtinId] : []
    );

    expect(new Set(registered).size).toBe(registered.length);
    expect(
      [...registered, ...Object.keys(NON_SCROLLBACK_TOOL_IDS)].sort()
    ).toEqual(Object.keys(TOOL_LABELS).sort());
  });

  it('resolves every registered name through its declared capability', () => {
    for (const capability of Object.values(TOOL_CAPABILITIES)) {
      const diffByName =
        'diffByName' in capability
          ? (capability.diffByName as Readonly<Record<string, ToolDiffPolicy>>)
          : undefined;
      const parentNames =
        'parentNames' in capability
          ? (capability.parentNames as readonly string[])
          : [];
      for (const name of capability.names) {
        expect(resolveScrollbackToolRenderer(name)).toBe(capability.renderer);
        expect(toolVerbosityPolicy(name)).toEqual(
          ('verbosity' in capability ? capability.verbosity : undefined) ??
            TOOL_RENDERER_VERBOSITY[capability.renderer]
        );
        const expectedDiff =
          capability.diff === 'none' ? 'none' : diffByName?.[name];
        if (expectedDiff === undefined) {
          throw new Error(`${name} needs an explicit diff decision`);
        }
        expect(toolDiffPolicy(name)).toBe(expectedDiff);
        const kinds = 'kinds' in capability ? capability.kinds : [];
        for (const kind of kinds) {
          expect(
            toolDiffPolicy(name, kind),
            `${name} must keep its explicit diff policy when ACP sends kind=${kind}`
          ).toBe(expectedDiff);
        }
        expect(toolApprovalPresentation(name)).toBe(
          capability.renderer === 'session' && parentNames.includes(name)
            ? 'subagent'
            : expectedDiff === 'unified'
              ? 'diff'
              : 'arguments'
        );
      }
      if (capability.diff !== 'none') {
        expect(Object.keys(diffByName ?? {}).sort()).toEqual(
          [...capability.names].sort()
        );
      }
    }
  });

  it('keeps unknown tools generic regardless of ACP kind metadata', () => {
    for (const kind of ['edit', 'read', 'execute', 'search']) {
      expect(resolveScrollbackToolRenderer('mcp__server__unknown', kind)).toBe(
        'generic'
      );
      expect(resolveToolId('mcp__server__unknown', kind)).toBeUndefined();
      expect(resolveToolDisplayName('mcp__server__unknown', kind)).toBe(
        'mcp__server__unknown'
      );
      expect(toolApprovalPresentation('mcp__server__unknown', kind)).toBe(
        'arguments'
      );
      expect(toolDiffPolicy('mcp__server__unknown', kind)).toBe('none');
      expect(isTrivialTool('mcp__server__unknown', kind)).toBe(false);
    }
  });

  it('keeps MCP tools generic when stripped KAS names collide with built-ins', () => {
    for (const [name, kind] of [
      ['fs_write', 'edit'],
      ['fs_read', 'read'],
      ['execute_bash', 'execute'],
      ['grep', 'search'],
      ['subagent', undefined],
      ['task', undefined],
    ] as const) {
      expect(resolveScrollbackToolRenderer(name, kind, 'mcp')).toBe('generic');
      expect(resolveToolId(name, kind, 'mcp')).toBeUndefined();
      expect(resolveToolDisplayName(name, kind, 'mcp')).toBe(name);
      expect(toolApprovalPresentation(name, kind, 'mcp')).toBe('arguments');
      expect(toolDiffPolicy(name, kind, 'mcp')).toBe('none');
      expect(isTrivialTool(name, kind, 'mcp')).toBe(false);
    }

    expect(resolveScrollbackToolRenderer('mcp__server__fs_write', 'edit')).toBe(
      'generic'
    );
    expect(toolDiffPolicy('mcp__server__fs_write', 'edit')).toBe('none');
  });

  it('falls back to kind only for genuinely unknown non-MCP titles', () => {
    for (const [name, kind, renderer, id, label] of [
      ['Inspect Workspace', 'read', 'read', 'read', 'Read'],
      ['Patch Workspace', 'edit', 'write', 'write', 'Write'],
      ['Run Workspace Command', 'execute', 'shell', 'shell', 'Shell'],
      ['Search Workspace', 'search', 'grep', 'grep', 'Grep'],
    ] as const) {
      expect(resolveScrollbackToolRenderer(name, kind)).toBe(renderer);
      expect(resolveToolId(name, kind)).toBe(id);
      expect(resolveToolDisplayName(name, kind)).toBe(label);
    }

    expect(toolApprovalPresentation('Patch Workspace', 'edit')).toBe('diff');
    expect(toolDiffPolicy('Patch Workspace', 'edit')).toBe('unified');
    expect(resolveScrollbackToolRenderer('fs_read', 'edit')).toBe('read');
  });

  it('preserves explicit legacy kind-first aliases', () => {
    expect(resolveScrollbackToolRenderer('Code Intelligence')).toBe('code');
    expect(resolveScrollbackToolRenderer('Code Intelligence', 'read')).toBe(
      'read'
    );
    expect(toolVerbosityPolicy('Code Intelligence', 'read')).toEqual({
      category: 'read',
    });
  });

  it('keeps legacy artifact aliases eligible for edit-kind diff rendering', () => {
    for (const name of ['Write', 'create', 'Edit', 'fs_edit']) {
      expect(resolveScrollbackToolRenderer(name, 'edit')).toBe('write');
      expect(toolDiffPolicy(name, 'edit')).toBe('unified');
      expect(toolApprovalPresentation(name, 'edit')).toBe('diff');
      expect(resolveScrollbackToolRenderer(name, 'edit', 'mcp')).toBe(
        'generic'
      );
      expect(toolDiffPolicy(name, 'edit', 'mcp')).toBe('none');
    }
  });

  it('does not let edit kind override registered non-diff write operations', () => {
    for (const name of [
      'fs_append',
      'delete_file',
      'Append to File',
      'Delete File',
    ]) {
      expect(toolDiffPolicy(name, 'edit')).toBe('none');
      expect(toolApprovalPresentation(name, 'edit')).toBe('arguments');
    }
  });

  it('uses specialized approval paint only for eligible registered names', () => {
    expect(toolApprovalPresentation('fs_write', 'edit')).toBe('diff');
    expect(toolApprovalPresentation('subagent')).toBe('subagent');
    expect(toolApprovalPresentation('orchestrate_subagent')).toBe('subagent');
    expect(toolApprovalPresentation('session_management')).toBe('arguments');
    expect(toolApprovalPresentation('subagent_response')).toBe('arguments');
  });

  it('centralizes special approval-detail aliases', () => {
    expect(toolApprovalDetail('execute_bash')).toBe('shell-command');
    expect(toolApprovalDetail('bash')).toBe('shell-command');
    expect(toolApprovalDetail('delete_file')).toBe('delete-path');
    expect(toolApprovalDetail('fs_delete')).toBe('delete-path');
    expect(toolApprovalDetail('custom_tool')).toBe('generic');
    expect(toolApprovalDetail('execute_bash', 'execute', 'mcp')).toBe(
      'generic'
    );
  });

  it('has an explicit verbosity policy for every renderer family', () => {
    expect(Object.keys(TOOL_RENDERER_VERBOSITY).sort()).toEqual(
      Object.keys(TOOL_RENDERER_APPROVAL).sort()
    );
  });
});
