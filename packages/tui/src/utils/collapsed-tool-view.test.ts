import { describe, it, expect } from 'bun:test';
import {
  collapsedToolPreview,
  isCollapsibleTool,
  resolveToolDisplayName,
} from './collapsed-tool-view.js';

describe('resolveToolDisplayName', () => {
  it('resolves builtin wire names to friendly labels', () => {
    expect(resolveToolDisplayName('str_replace')).toBe('Write');
    expect(resolveToolDisplayName('run_command')).toBe('Shell');
  });

  it('resolves a name that is itself a builtin id', () => {
    expect(resolveToolDisplayName('subagent')).toBe('Subagent');
    expect(resolveToolDisplayName('knowledge')).toBe('Knowledge');
  });

  it('falls back to ACP kind when the name is unknown', () => {
    expect(resolveToolDisplayName('custom_reader', 'read')).toBe('Read');
    expect(resolveToolDisplayName('custom_editor', 'edit')).toBe('Write');
  });

  it('falls back to the raw name when neither resolves', () => {
    expect(resolveToolDisplayName('mystery_tool')).toBe('mystery_tool');
    expect(resolveToolDisplayName('mystery_tool', 'unmapped')).toBe(
      'mystery_tool'
    );
  });
});

describe('collapsedToolPreview', () => {
  it('previews the first line of a subagent prompt and names the agent', () => {
    const content = JSON.stringify({
      agent: 'requirement-detailer',
      prompt: 'Requirement 2: View timeline\n\nDetail the acceptance criteria…',
    });
    expect(collapsedToolPreview('subagent', undefined, content)).toEqual({
      title: 'Subagent',
      target: 'requirement-detailer',
      preview: 'Requirement 2: View timeline',
    });
  });

  it('previews the first line of a generic tool primary arg', () => {
    const content = JSON.stringify({ command: 'npm run build\n--verbose' });
    expect(collapsedToolPreview('run_command', undefined, content)).toEqual({
      title: 'Shell',
      preview: 'npm run build',
    });
  });

  it('truncates long preview lines', () => {
    const long = 'x'.repeat(200);
    const { preview } = collapsedToolPreview(
      'run_command',
      undefined,
      JSON.stringify({ command: long })
    );
    expect(preview).toHaveLength(120);
    expect(preview!.endsWith('…')).toBe(true);
  });

  it('previews a subagent invocation (free-form title name) from its prompt first line', () => {
    const content = JSON.stringify({
      name: 'feature-requirements-first-workflow',
      prompt:
        'Spec type: New Feature. Feature name: spotify-clone. Workflow: Requirements-First.\n\nThe user wants to start a new spec...',
      explanation: 'Delegating to the requirements-first workflow.',
      preset: 'requirements',
    });
    expect(
      collapsedToolPreview(
        'Sub-agent: feature-requirements-first-workflow',
        'other',
        content
      )
    ).toEqual({
      title: 'Sub-agent: feature-requirements-first-workflow',
      preview:
        'Spec type: New Feature. Feature name: spotify-clone. Workflow: Requirements-First.',
    });
  });

  it('omits preview when there is no recognized arg', () => {
    expect(collapsedToolPreview('goal', undefined, '{}')).toEqual({
      title: 'goal',
      preview: undefined,
    });
  });

  it('skips leading blank lines when previewing', () => {
    const content = JSON.stringify({ prompt: '\n\n  First real line\nmore' });
    const { preview } = collapsedToolPreview('subagent', undefined, content);
    expect(preview).toBe('First real line');
  });

  it('surfaces the fs_read operations[].path when there is no top-level arg', () => {
    const content = JSON.stringify({
      operations: [{ mode: 'Line', path: '/src/index.ts' }],
    });
    const { preview } = collapsedToolPreview('fs_read', 'read', content);
    expect(preview).toBe('/src/index.ts');
  });

  it('falls back to the legacy `ops` array and the first image path', () => {
    const content = JSON.stringify({
      ops: [{ mode: 'Image', image_paths: ['/img/a.png', '/img/b.png'] }],
    });
    const { preview } = collapsedToolPreview('fs_read', 'read', content);
    expect(preview).toBe('/img/a.png');
  });
});

describe('isCollapsibleTool', () => {
  it('collapses tools that carry args (recognized, MCP/unknown, delegations)', () => {
    expect(
      isCollapsibleTool(JSON.stringify({ command: 'npm run build' }))
    ).toBe(true);
    expect(isCollapsibleTool(JSON.stringify({ path: '/tmp/x.ts' }))).toBe(true);
    // Spec subagent and any (non-spec) delegation both collapse now.
    expect(
      isCollapsibleTool(
        JSON.stringify({
          name: 'feature-requirements-first-workflow',
          prompt: 'x',
        })
      )
    ).toBe(true);
    expect(
      isCollapsibleTool(
        JSON.stringify({ name: 'my-custom-agent', prompt: 'x' })
      )
    ).toBe(true);
    // MCP / unknown tool with arbitrary args.
    expect(isCollapsibleTool(JSON.stringify({ foo: 'bar' }))).toBe(true);
  });

  it('does NOT collapse arg-less calls (KAS user_input: question in the title)', () => {
    // KAS emits user_input with no rawInput — nothing to collapse; rendered
    // normally so the title stays markdown-honored instead of a plain line.
    expect(isCollapsibleTool('{}')).toBe(false);
    expect(isCollapsibleTool(undefined)).toBe(false);
    expect(isCollapsibleTool('')).toBe(false);
  });

  it('does NOT collapse interactive user_input questions', () => {
    // A user_input question carries `question` (no `prompt`) — it must stay
    // visible/markdown-rendered, not collapse to an empty expand affordance.
    expect(
      isCollapsibleTool(
        JSON.stringify({
          question: '**Is this a new feature or a bugfix?**',
          options: [{ title: 'Feature' }, { title: 'Bugfix' }],
        })
      )
    ).toBe(false);
    // Free-text question (no options) also stays visible.
    expect(isCollapsibleTool(JSON.stringify({ question: 'What next?' }))).toBe(
      false
    );
  });

  it('treats a question that also carries a prompt as a delegation (collapses)', () => {
    expect(
      isCollapsibleTool(JSON.stringify({ question: 'q', prompt: 'do it' }))
    ).toBe(true);
  });
});
