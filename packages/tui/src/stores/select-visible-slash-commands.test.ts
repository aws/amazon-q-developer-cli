import { describe, it, expect, mock, afterAll } from 'bun:test';
import { createAppStore } from './app-store';
import { selectVisibleSlashCommands } from './selectors';
import { Kiro } from '../kiro';
import { KAS_COMMANDS } from '../kas-commands';
import { features } from '../features';

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
      // cloud-only commands (e.g. /repo) are gated out of a non-cloud session.
      if (cmd.meta?.cloudOnly) {
        expect(visible.find((c) => c.name === cmd.name)).toBeUndefined();
        continue;
      }
      // feature-gated commands (e.g. /tangent) are hidden unless the
      // launcher-provided KIRO_ENABLED_FEATURES lists them; tests run with
      // no features enabled.
      if (cmd.feature && !features.isEnabled(cmd.feature)) {
        expect(visible.find((c) => c.name === cmd.name)).toBeUndefined();
        continue;
      }
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
    store.getState().setSteering([
      {
        name: 'project-context',
        telemetryId: 'workflow-run',
        source: { kind: 'workspace' },
      },
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
    expect(steering?.meta?.telemetryId).toBe('workflow-run');
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

describe('/repo cloud-only visibility gate (dark-ship)', () => {
  it('hides /repo in a non-cloud KAS session', () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    // cloudSessionActive defaults to false (a local session / released build).
    const visible = selectVisibleSlashCommands(store.getState());
    expect(visible.find((c) => c.name === '/repo')).toBeUndefined();
  });

  it('shows /repo once the session is marked cloud', () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    store.getState().setCloudSessionActive(true);
    const visible = selectVisibleSlashCommands(store.getState());
    expect(visible.find((c) => c.name === '/repo')).toBeDefined();
  });

  it('hides /sessions from a local session', () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    // cloudSessionActive defaults to false (a local session / released build).
    const visible = selectVisibleSlashCommands(store.getState());
    expect(visible.find((c) => c.name === '/sessions')).toBeUndefined();
  });

  it('shows /sessions once the session is marked cloud', () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    store.getState().setCloudSessionActive(true);
    const visible = selectVisibleSlashCommands(store.getState());
    expect(visible.find((c) => c.name === '/sessions')).toBeDefined();
  });

  it('never shows /repo in v2 mode even with the cloud flag set', () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'v2' });
    store.getState().setCloudSessionActive(true);
    const visible = selectVisibleSlashCommands(store.getState());
    expect(visible.find((c) => c.name === '/repo')).toBeUndefined();
  });
});

describe('/autonomous feature + cloud visibility gates (dark-ship)', () => {
  const withFeatures = (value: string | undefined, fn: () => void) => {
    const prev = process.env.KIRO_ENABLED_FEATURES;
    if (value === undefined) delete process.env.KIRO_ENABLED_FEATURES;
    else process.env.KIRO_ENABLED_FEATURES = value;
    features._resetForTests();
    try {
      fn();
    } finally {
      if (prev === undefined) delete process.env.KIRO_ENABLED_FEATURES;
      else process.env.KIRO_ENABLED_FEATURES = prev;
      features._resetForTests();
    }
  };

  it('is absent without the remote_sandbox feature, even in a cloud session', () => {
    withFeatures('[]', () => {
      const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
      store.getState().setCloudSessionActive(true);
      const visible = selectVisibleSlashCommands(store.getState());
      expect(visible.find((c) => c.name === '/autonomous')).toBeUndefined();
    });
  });

  it('is absent outside cloud sessions even with the feature enabled', () => {
    withFeatures('["remote_sandbox"]', () => {
      const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
      const visible = selectVisibleSlashCommands(store.getState());
      expect(visible.find((c) => c.name === '/autonomous')).toBeUndefined();
    });
  });

  it('is visible with the feature enabled inside a cloud session', () => {
    withFeatures('["remote_sandbox"]', () => {
      const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
      store.getState().setCloudSessionActive(true);
      const visible = selectVisibleSlashCommands(store.getState());
      expect(visible.find((c) => c.name === '/autonomous')).toBeDefined();
    });
  });
});

describe('v2Only commands — hidden in KAS, visible in V2', () => {
  it('hides /theme from autocomplete in KAS mode (hidden flag)', () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    const visible = selectVisibleSlashCommands(store.getState());
    const theme = visible.find((c) => c.name === '/theme');
    expect(theme).toBeDefined();
    expect(theme!.meta?.hidden).toBe(true);
  });

  it('hides /verbosity from autocomplete in KAS mode (hidden flag)', () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    const visible = selectVisibleSlashCommands(store.getState());
    const verbosity = visible.find((c) => c.name === '/verbosity');
    expect(verbosity).toBeDefined();
    expect(verbosity!.meta?.hidden).toBe(true);
  });

  it('shows /theme in autocomplete in V2 mode (not hidden)', () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'v2' });
    const visible = selectVisibleSlashCommands(store.getState());
    const theme = visible.find((c) => c.name === '/theme');
    expect(theme).toBeDefined();
    expect(theme!.meta?.hidden).not.toBe(true);
  });

  it('keeps /settings visible in KAS mode', () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    const visible = selectVisibleSlashCommands(store.getState());
    expect(visible.find((c) => c.name === '/settings')).toBeDefined();
  });

  it('/theme and /verbosity remain dispatchable in KAS (exact-match lookup)', () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
    const visible = selectVisibleSlashCommands(store.getState());
    // hidden commands are excluded from prefix match but findable by exact name
    expect(visible.find((c) => c.name === '/theme')).toBeDefined();
    expect(visible.find((c) => c.name === '/verbosity')).toBeDefined();
  });
});

describe('/workflow* local-only visibility gate (cloud stopgap, kiro-agent #178)', () => {
  const withFeatures = (value: string | undefined, fn: () => void) => {
    const prev = process.env.KIRO_ENABLED_FEATURES;
    if (value === undefined) delete process.env.KIRO_ENABLED_FEATURES;
    else process.env.KIRO_ENABLED_FEATURES = value;
    features._resetForTests();
    try {
      fn();
    } finally {
      if (prev === undefined) delete process.env.KIRO_ENABLED_FEATURES;
      else process.env.KIRO_ENABLED_FEATURES = prev;
      features._resetForTests();
    }
  };

  const gated = ['/workflow', '/goal', '/workflows'];

  it('shows /workflow + /goal in a local KAS session (feature enabled)', () => {
    withFeatures('["workflows"]', () => {
      const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
      // cloudSessionActive defaults to false (a local session).
      const visible = selectVisibleSlashCommands(store.getState());
      expect(visible.find((c) => c.name === '/workflow')).toBeDefined();
      expect(visible.find((c) => c.name === '/goal')).toBeDefined();
    });
  });

  it('hides all /workflow* + /goal once the session is marked cloud', () => {
    withFeatures('["workflows"]', () => {
      const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
      store.getState().setCloudSessionActive(true);
      const visible = selectVisibleSlashCommands(store.getState());
      for (const name of gated) {
        expect(visible.find((c) => c.name === name)).toBeUndefined();
      }
    });
  });
});

describe('UI mode commands — KAS vs V2', () => {
  const withRollout = (value: string | undefined, fn: () => void) => {
    const prev = process.env.KIRO_LITE_ROLLOUT_ENABLED;
    if (value === undefined) delete process.env.KIRO_LITE_ROLLOUT_ENABLED;
    else process.env.KIRO_LITE_ROLLOUT_ENABLED = value;
    try {
      fn();
    } finally {
      if (prev === undefined) delete process.env.KIRO_LITE_ROLLOUT_ENABLED;
      else process.env.KIRO_LITE_ROLLOUT_ENABLED = prev;
    }
  };

  // /tui must stay visible in both engines — KAS lite↔TUI switching dispatches
  // it through this list (liteGateCommands), so filtering it breaks the swap.
  for (const agentEngine of ['kas', 'v2'] as const) {
    it(`keeps /tui visible for ${agentEngine} mode switching`, () => {
      const cmd = selectVisibleSlashCommands(
        createAppStore({ kiro: new Kiro(), agentEngine }).getState()
      ).find((c) => c.name === '/tui');
      expect(cmd?.description).toContain('Switch to TUI mode');
    });

    // /lite is the same swap entry but gated on the rollout.
    it(`gates /lite on the rollout for ${agentEngine} mode switching`, () => {
      withRollout('1', () => {
        const cmd = selectVisibleSlashCommands(
          createAppStore({ kiro: new Kiro(), agentEngine }).getState()
        ).find((c) => c.name === '/lite');
        expect(cmd?.description).toBe('[EXPERIMENTAL] Switch to Lite UI');
      });
      withRollout(undefined, () => {
        const cmd = selectVisibleSlashCommands(
          createAppStore({ kiro: new Kiro(), agentEngine }).getState()
        ).find((c) => c.name === '/lite');
        expect(cmd).toBeUndefined();
      });
    });
  }
});
