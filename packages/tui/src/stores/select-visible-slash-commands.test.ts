import { describe, it, expect, mock, afterAll } from 'bun:test';
import { createAppStore } from './app-store';
import { selectVisibleSlashCommands } from './selectors';
import { Kiro } from '../kiro';
import { KAS_COMMANDS } from '../kas-commands';

mock.module('../kiro', () => ({
  Kiro: mock(() => ({
    sendMessageStream: mock(),
    cancel: mock(),
    close: mock(),
  })),
}));

afterAll(() => {
  mock.restore();
});

describe('selectVisibleSlashCommands', () => {
  it("returns KAS commands plus slashCommands when agentEngine === 'kas'", () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    const visible = selectVisibleSlashCommands(store.getState());
    for (const cmd of KAS_COMMANDS) {
      expect(visible.find((c) => c.name === cmd.name)).toBeDefined();
    }
    for (const cmd of store.getState().slashCommands) {
      expect(visible.find((c) => c.name === cmd.name)).toBeDefined();
    }
  });

  it('exposes KAS-broadcast commands in autocomplete (regression: filter hid them)', () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    // KAS broadcasts via setSlashCommands with source='backend' (see
    // index.tsx onCommandsUpdate). These must remain visible.
    store
      .getState()
      .setSlashCommands([
        { name: '/kas-prompt', description: 'Prompt', source: 'backend' },
      ]);
    const visible = selectVisibleSlashCommands(store.getState());
    expect(visible.find((c) => c.name === '/kas-prompt')).toBeDefined();
  });

  it("returns slashCommands plus prompt/skill/steering projections in 'v2' mode", () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'v2' });
    const result = selectVisibleSlashCommands(store.getState());
    // Empty slices: result is just the seeded host commands.
    expect(result).toEqual([...store.getState().slashCommands]);
  });

  it('keeps host-side commands like /exit and /settings reachable in KAS mode', () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    const visible = selectVisibleSlashCommands(store.getState());
    expect(visible.find((c) => c.name === '/quit')).toBeDefined();
    expect(visible.find((c) => c.name === '/exit')).toBeDefined();
    expect(visible.find((c) => c.name === '/settings')).toBeDefined();
  });

  it('exposes /feedback in KAS mode', () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    const visible = selectVisibleSlashCommands(store.getState());
    expect(visible.find((c) => c.name === '/feedback')).toBeDefined();
  });

  it('merges prompts, skills, and steering into the visible list', () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'v2' });
    store.getState().setPrompts([
      {
        name: 'research',
        arguments: [{ name: 'topic', required: true }],
        source: { kind: 'mcp', serverName: 'core' },
      },
    ]);
    store.getState().setSkills([
      {
        name: 'agent-sop',
        source: { kind: 'agent-config', path: '/path/to/sop.md' },
      },
    ]);
    store
      .getState()
      .setSteering([
        { name: 'project-context', source: { kind: 'workspace' } },
      ]);
    const visible = selectVisibleSlashCommands(store.getState());

    const prompt = visible.find((c) => c.name === '/research');
    expect(prompt?.meta?.type).toBe('prompt');
    expect(prompt?.meta?.source).toEqual({
      kind: 'mcp',
      serverName: 'core',
    });

    const skill = visible.find((c) => c.name === '/agent-sop');
    expect(skill?.meta?.type).toBe('skill');
    expect(skill?.meta?.source).toEqual({
      kind: 'agent-config',
      path: '/path/to/sop.md',
    });

    const steering = visible.find((c) => c.name === '/project-context');
    expect(steering?.meta?.type).toBe('steering');
  });

  it('does NOT duplicate prompts/skills/steering into AppState.slashCommands', () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'v2' });
    store.getState().setPrompts([
      {
        name: 'research',
        arguments: [],
        source: { kind: 'workspace' },
      },
    ]);
    store.getState().setSkills([
      {
        name: 'sop',
        source: { kind: 'agent-config', path: '/p' },
      },
    ]);
    // The slice itself only contains host commands -- prompts/skills don't
    // leak into it.
    const slashCommands = store.getState().slashCommands;
    expect(slashCommands.find((c) => c.name === '/research')).toBeUndefined();
    expect(slashCommands.find((c) => c.name === '/sop')).toBeUndefined();
    // But the merged visible list includes them.
    const visible = selectVisibleSlashCommands(store.getState());
    expect(visible.find((c) => c.name === '/research')).toBeDefined();
    expect(visible.find((c) => c.name === '/sop')).toBeDefined();
  });

  it('renders skill descriptions', () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'v2' });
    store.getState().setSkills([
      {
        name: 'with-description',
        description: 'Deep research on a topic',
        source: { kind: 'agent-config', path: 'config' },
      },
      { name: 'workspace-fallback', source: { kind: 'workspace' } },
      { name: 'global-fallback', source: { kind: 'global' } },
      { name: 'agent-config-fallback', source: { kind: 'agent-config' } },
    ]);
    const visible = selectVisibleSlashCommands(store.getState());
    const desc = (n: string) => visible.find((c) => c.name === n)?.description;
    expect(desc('/with-description')).toBe('Deep research on a topic');
    expect(desc('/workspace-fallback')).toBe('Skill from workspace');
    expect(desc('/global-fallback')).toBe('Skill from global');
    expect(desc('/agent-config-fallback')).toBe('Skill from agent config');
  });

  it('dedupes by name; first occurrence wins', () => {
    // Regression: V2 ships built-in `/plan` as a slash command AND users
    // can have a workspace skill or prompt named `plan`. Without dedup
    // the merged list contains two `/plan` entries which causes a React
    // duplicate-key warning in the autocomplete menu.
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'v2' });
    store
      .getState()
      .setSlashCommands([
        { name: '/plan', description: 'Built-in plan', source: 'local' },
      ]);
    store
      .getState()
      .setPrompts([
        { name: 'plan', arguments: [], source: { kind: 'workspace' } },
      ]);
    store.getState().setSkills([{ name: 'plan', source: { kind: 'global' } }]);
    const visible = selectVisibleSlashCommands(store.getState());
    const planEntries = visible.filter((c) => c.name === '/plan');
    expect(planEntries).toHaveLength(1);
    expect(planEntries[0]!.description).toBe('Built-in plan');
  });

  it('dedupes prompts vs skills with the same name; prompt wins', () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'v2' });
    store
      .getState()
      .setPrompts([
        { name: 'review', arguments: [], source: { kind: 'workspace' } },
      ]);
    store
      .getState()
      .setSkills([{ name: 'review', source: { kind: 'global' } }]);
    const visible = selectVisibleSlashCommands(store.getState());
    const matches = visible.filter((c) => c.name === '/review');
    expect(matches).toHaveLength(1);
    expect(matches[0]!.meta?.type).toBe('prompt');
  });
});

describe('/tui filter — KAS vs V2', () => {
  it("when agentEngine is 'kas', /tui is NOT in slashCommands", () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    const visible = selectVisibleSlashCommands(store.getState());
    const tui = visible.find((c) => c.name === '/tui');
    expect(tui).toBeUndefined();
  });

  it("when agentEngine is 'v2', /tui IS in slashCommands", () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'v2' });
    const visible = selectVisibleSlashCommands(store.getState());
    const tui = visible.find((c) => c.name === '/tui');
    expect(tui).toBeDefined();
    expect(tui!.description).toContain("What's new");
  });

  it("when agentEngine is 'kas', /tui is not in the raw slashCommands slice", () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    const raw = store.getState().slashCommands;
    expect(raw.find((c) => c.name === '/tui')).toBeUndefined();
  });

  it("when agentEngine is 'v2', /tui is in the raw slashCommands slice", () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'v2' });
    const raw = store.getState().slashCommands;
    expect(raw.find((c) => c.name === '/tui')).toBeDefined();
  });
});
