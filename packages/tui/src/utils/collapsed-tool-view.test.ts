import { describe, it, expect } from 'bun:test';
import {
  collapsedToolPreview,
  isCollapsibleTool,
  isSubagentCard,
  resolveToolDisplayName,
  shouldCollapseToolCard,
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
    // KAS "Sub-agent: <role>" wrapper: title normalizes to "Subagent", the role
    // moves to the target, and the preview is the prompt's first line.
    expect(
      collapsedToolPreview(
        'Sub-agent: feature-requirements-first-workflow',
        'other',
        content
      )
    ).toEqual({
      title: 'Subagent',
      target: 'feature-requirements-first-workflow',
      preview:
        'Spec type: New Feature. Feature name: spotify-clone. Workflow: Requirements-First.',
    });
  });

  it('names the agent (target) from a KAS "Sub-agent: <role>" wrapper and previews its prompt', () => {
    // Matches the ACP wrapper shape: kind 'other', rawInput { name, prompt }.
    const content = JSON.stringify({
      name: 'general-task-execution',
      prompt: 'Investigate the failing build\nthen report back with the cause',
    });
    expect(
      collapsedToolPreview(
        'Sub-agent: general-task-execution',
        'other',
        content
      )
    ).toEqual({
      title: 'Subagent',
      target: 'general-task-execution',
      preview: 'Investigate the failing build',
    });
  });

  it('omits the target for a roleless "Sub-agent:" title but still previews the prompt', () => {
    const content = JSON.stringify({ prompt: 'do the thing\nand more' });
    expect(collapsedToolPreview('Sub-agent:', 'other', content)).toEqual({
      title: 'Subagent',
      target: undefined,
      preview: 'do the thing',
    });
  });

  it('previews an "Orchestrate Sub-agent" card and derives its target from the agent arg', () => {
    const content = JSON.stringify({
      subagent_type: 'requirements-first-workflow',
      prompt: 'Kick off the spec workflow\nfor the new feature',
      explanation: 'Orchestrating the spec pipeline.',
    });
    expect(
      collapsedToolPreview('Orchestrate Sub-agent', 'other', content)
    ).toEqual({
      title: 'Orchestrate Sub-agent',
      target: 'requirements-first-workflow',
      preview: 'Kick off the spec workflow',
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

describe('isSubagentCard', () => {
  it('is true for KAS title-form spawn cards (any role + orchestration)', () => {
    expect(isSubagentCard('Sub-agent: general-task-execution', 'other')).toBe(
      true
    );
    expect(isSubagentCard('Sub-agent: requirement-detailer', 'other')).toBe(
      true
    );
    expect(isSubagentCard('Orchestrate Sub-agent', 'other')).toBe(true);
  });

  it('is false for snake_case spawn wire names (they route to SessionTool)', () => {
    // These already render clean live labels via SessionTool (agent counts,
    // "Spawned agent", shimmer); intercepting them here regresses that to a
    // raw collapsed title, so isSubagentCard must NOT claim them.
    expect(isSubagentCard('subagent')).toBe(false);
    expect(isSubagentCard('agent_crew')).toBe(false);
    expect(isSubagentCard('orchestrate_subagent')).toBe(false);
    expect(isSubagentCard('invoke_sub_agent')).toBe(false);
    expect(isSubagentCard('Invoke Agent')).toBe(false);
  });

  it('is false for a title-form card with a non-"other" kind', () => {
    // The kind guard avoids a false positive on a coincidental MCP/user tool
    // that happens to share the title text.
    expect(isSubagentCard('Sub-agent: general-task-execution', 'execute')).toBe(
      false
    );
    expect(isSubagentCard('Orchestrate Sub-agent', 'read')).toBe(false);
  });

  it('is false for non-spawn session tools', () => {
    // These must keep their normal render — they are not subagent spawns.
    expect(isSubagentCard('session_management')).toBe(false);
    expect(isSubagentCard('subagent_response')).toBe(false);
    expect(isSubagentCard('Subagent Response')).toBe(false);
  });

  it('is false for user_input and plain tools', () => {
    expect(isSubagentCard('user_input')).toBe(false);
    expect(isSubagentCard('run_command')).toBe(false);
    expect(isSubagentCard('fs_read', 'read')).toBe(false);
    expect(isSubagentCard('mystery_tool')).toBe(false);
  });
});

describe('shouldCollapseToolCard', () => {
  const withArgs = '{"prompt":"do the thing"}';

  it('collapses a subagent title-form card in any mode (not hideArgs)', () => {
    expect(
      shouldCollapseToolCard(
        'Sub-agent: general-task-execution',
        'other',
        withArgs,
        false
      )
    ).toBe(true);
    expect(
      shouldCollapseToolCard('Orchestrate Sub-agent', 'other', withArgs, false)
    ).toBe(true);
  });

  it('collapses any collapsible tool when spec mode hides args (hideArgs)', () => {
    expect(
      shouldCollapseToolCard('fs_read', 'read', '{"path":"/a"}', true)
    ).toBe(true);
  });

  it('does NOT collapse snake_case crew/spawn names outside spec mode (they keep SessionTool rendering)', () => {
    // Regression guard for the SessionTool divert: these route to SessionTool
    // (live "Orchestrating (N agents)"/"Spawned agent" labels), not a collapsed
    // raw-title preview.
    for (const name of [
      'subagent',
      'agent_crew',
      'orchestrate_subagent',
      'invoke_sub_agent',
      'Invoke Agent',
    ]) {
      expect(shouldCollapseToolCard(name, 'other', withArgs, false)).toBe(
        false
      );
    }
  });

  it('does NOT collapse a plain tool outside spec mode', () => {
    expect(
      shouldCollapseToolCard('fs_read', 'read', '{"path":"/a"}', false)
    ).toBe(false);
  });

  it('does NOT collapse an arg-less card even if it is a subagent card', () => {
    expect(shouldCollapseToolCard('Sub-agent: x', 'other', '{}', false)).toBe(
      false
    );
    expect(
      shouldCollapseToolCard('Sub-agent: x', 'other', undefined, false)
    ).toBe(false);
  });

  it('does NOT collapse a user_input question even in spec mode', () => {
    expect(
      shouldCollapseToolCard(
        'user_input',
        'other',
        '{"question":"Pick one?"}',
        true
      )
    ).toBe(false);
  });

  it('respects the kind guard: a non-other-kind "Sub-agent:" card is not collapsed outside spec mode', () => {
    expect(
      shouldCollapseToolCard('Sub-agent: x', 'execute', withArgs, false)
    ).toBe(false);
  });
});
