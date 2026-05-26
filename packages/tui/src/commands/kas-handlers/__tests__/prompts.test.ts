import { describe, it, expect } from 'bun:test';
import { handlePrompts } from '../prompts';
import { createMockCommandContext } from '../../__tests__/test-helpers';
import type { KasCommand } from '../../../kas-commands';
import { KasCommandName } from '../../../kas-commands';
import type {
  PromptEntry,
  SkillEntry,
  SteeringEntry,
} from '../../../types/commands';

const PROMPTS_CMD: KasCommand = {
  name: KasCommandName.Prompts,
  description: 'Select or list available prompts',
  meta: { inputType: 'selection' },
};

const wsPrompt = (name: string): PromptEntry => ({
  name,
  description: `${name} desc`,
  arguments: [],
  source: { kind: 'workspace' },
});

const mcpPrompt = (name: string, server: string): PromptEntry => ({
  name,
  description: `${name} desc`,
  arguments: [{ name: 'topic', required: true }],
  source: { kind: 'mcp', serverName: server },
});

const wsSkill = (name: string): SkillEntry => ({
  name,
  description: `${name} skill`,
  source: { kind: 'workspace' },
});

const wsSteering = (name: string): SteeringEntry => ({
  name,
  description: `${name} steering`,
  source: { kind: 'workspace' },
});

describe('handlePrompts (KAS-mode dispatch)', () => {
  describe('open picker (no args)', () => {
    it('combines prompts, skills, and steering into one option list', async () => {
      const ctx = createMockCommandContext({
        kasCommands: [PROMPTS_CMD],
        prompts: [wsPrompt('summarize')],
        skills: [wsSkill('tdd')],
        steering: [wsSteering('conventions')],
      });
      await handlePrompts(PROMPTS_CMD, '', ctx);
      const setActive = ctx._spies.setActiveCommand as any;
      expect(setActive).toHaveBeenCalledTimes(1);
      const opts = setActive.mock.calls[0][0].options;
      expect(opts.map((o: any) => o.value)).toEqual(
        // sort: group asc (skill, steering, workspace), label asc within
        ['tdd', 'conventions', 'summarize']
      );
    });

    it('groups MCP-scoped prompts by their server name', async () => {
      const ctx = createMockCommandContext({
        kasCommands: [PROMPTS_CMD],
        prompts: [mcpPrompt('search', 'github'), wsPrompt('review')],
        skills: [],
        steering: [],
      });
      await handlePrompts(PROMPTS_CMD, '', ctx);
      const setActive = ctx._spies.setActiveCommand as any;
      const opts = setActive.mock.calls[0][0].options;
      const search = opts.find((o: any) => o.value === 'search');
      const review = opts.find((o: any) => o.value === 'review');
      expect(search.group).toBe('github');
      expect(review.group).toBe('workspace');
    });

    it('uses entity type as group label for skills and steering', async () => {
      const ctx = createMockCommandContext({
        kasCommands: [PROMPTS_CMD],
        prompts: [],
        skills: [wsSkill('tdd')],
        steering: [wsSteering('plan')],
      });
      await handlePrompts(PROMPTS_CMD, '', ctx);
      const opts = (ctx._spies.setActiveCommand as any).mock.calls[0][0]
        .options;
      expect(opts.find((o: any) => o.value === 'tdd').group).toBe('skill');
      expect(opts.find((o: any) => o.value === 'plan').group).toBe('steering');
    });

    it('formats arg hints only for prompts', async () => {
      const ctx = createMockCommandContext({
        kasCommands: [PROMPTS_CMD],
        prompts: [mcpPrompt('search', 'github')],
        skills: [wsSkill('tdd')],
        steering: [],
      });
      await handlePrompts(PROMPTS_CMD, '', ctx);
      const opts = (ctx._spies.setActiveCommand as any).mock.calls[0][0]
        .options;
      expect(opts.find((o: any) => o.value === 'search').hint).toBe('<topic>');
      expect(opts.find((o: any) => o.value === 'tdd').hint).toBeUndefined();
    });

    it('alerts when nothing is advertised', async () => {
      const ctx = createMockCommandContext({
        kasCommands: [PROMPTS_CMD],
        prompts: [],
        skills: [],
        steering: [],
      });
      await handlePrompts(PROMPTS_CMD, '', ctx);
      expect(ctx._spies.setActiveCommand).not.toHaveBeenCalled();
      const showAlert = ctx._spies.showAlert as any;
      expect(showAlert).toHaveBeenCalledWith(
        'No prompts, skills, or steering available',
        'error',
        3000
      );
    });
  });

  describe('synthetic dispatch (with arg)', () => {
    it('sends `/<name>` as a chat message when an entry is picked', async () => {
      const ctx = createMockCommandContext({
        kasCommands: [PROMPTS_CMD],
      });
      await handlePrompts(PROMPTS_CMD, 'summarize', ctx);
      const sendMessage = ctx._spies.sendMessage as any;
      expect(sendMessage).toHaveBeenCalledWith('/summarize');
      expect(ctx._spies.setActiveCommand).not.toHaveBeenCalled();
    });
  });
});
