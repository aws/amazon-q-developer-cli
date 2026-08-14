import {
  describe,
  it,
  expect,
  mock,
  afterAll,
  beforeEach,
  afterEach,
} from 'bun:test';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import { createAppStore } from './app-store';
import { selectVisibleSlashCommands } from './selectors';
import { Kiro } from '../kiro';
import { KAS_COMMANDS } from '../kas-commands';
import { features, Feature } from '../features';

// mock.module is process-global and survives this file — snapshot the real
// modules and re-register them afterAll so mocks cannot leak into other files.
import { restoreRealModulesAfterAll } from '../test-utils/restore-modules.js';

restoreRealModulesAfterAll(import.meta.dir, ['../kiro']);

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

/**
 * Command visibility is resolved from the rollout AND, for workflows, a
 * persisted opt-in — so an inherited `KIRO_ENABLED_FEATURES` or the
 * developer's own `cli.json` would otherwise decide these assertions. Both
 * are pinned here rather than assumed: the cases below state the features
 * they need, and a scratch home keeps real preferences out of the result.
 */
let envHome: string | undefined;
let envFeatures: string | undefined;
let scratchHome: string;

beforeEach(() => {
  envHome = process.env.KIRO_HOME;
  envFeatures = process.env.KIRO_ENABLED_FEATURES;
  scratchHome = mkdtempSync(join(tmpdir(), 'slash-visibility-base-'));
  mkdirSync(join(scratchHome, 'settings'), { recursive: true });
  writeFileSync(join(scratchHome, 'settings', 'cli.json'), '{}');
  process.env.KIRO_HOME = scratchHome;
  process.env.KIRO_ENABLED_FEATURES = '[]';
  features._resetForTests();
});

afterEach(() => {
  if (envHome === undefined) delete process.env.KIRO_HOME;
  else process.env.KIRO_HOME = envHome;
  if (envFeatures === undefined) delete process.env.KIRO_ENABLED_FEATURES;
  else process.env.KIRO_ENABLED_FEATURES = envFeatures;
  features._resetForTests();
  rmSync(scratchHome, { recursive: true, force: true });
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
      // Feature-gated commands (e.g. /tangent) are hidden unless the
      // launcher-provided KIRO_ENABLED_FEATURES lists them; the harness
      // pins that to none. Workflow commands take a persisted opt-in on top
      // of the rollout, so being on the rollout alone does not reveal them.
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

describe('/voice runtime availability gate', () => {
  const withVoiceEnvironment = (
    supported: string,
    serverUrl: string | undefined,
    assertion: () => void
  ) => {
    const previousSupported = process.env.KIRO_VOICE_SUPPORTED;
    const previousServerUrl = process.env.KIRO_VOICE_SERVER_URL;
    process.env.KIRO_VOICE_SUPPORTED = supported;
    if (serverUrl === undefined) delete process.env.KIRO_VOICE_SERVER_URL;
    else process.env.KIRO_VOICE_SERVER_URL = serverUrl;
    try {
      assertion();
    } finally {
      if (previousSupported === undefined)
        delete process.env.KIRO_VOICE_SUPPORTED;
      else process.env.KIRO_VOICE_SUPPORTED = previousSupported;
      if (previousServerUrl === undefined)
        delete process.env.KIRO_VOICE_SERVER_URL;
      else process.env.KIRO_VOICE_SERVER_URL = previousServerUrl;
      features._resetForTests();
    }
  };

  const visibleVoiceCommand = () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'v2' });
    store
      .getState()
      .setSlashCommands([
        { name: '/voice', description: 'Record voice input', source: 'local' },
      ]);
    return selectVisibleSlashCommands(store.getState()).find(
      (command) => command.name === '/voice'
    );
  };

  it('hides /voice without local or remote support', () => {
    withVoiceEnvironment('0', undefined, () => {
      expect(visibleVoiceCommand()).toBeUndefined();
    });
  });

  it('shows /voice with a remote server in a build without local support', () => {
    withVoiceEnvironment('0', 'http://127.0.0.1:19876', () => {
      expect(visibleVoiceCommand()).toBeDefined();
    });
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

  it('never shows /repo in v2 mode even with the cloud flag set', () => {
    const store = createAppStore({ kiro: new Kiro(), agentEngine: 'v2' });
    store.getState().setCloudSessionActive(true);
    const visible = selectVisibleSlashCommands(store.getState());
    expect(visible.find((c) => c.name === '/repo')).toBeUndefined();
  });
});

describe('/sessions nightly feature gate', () => {
  // getKasCommands() reads the feature set at store-creation time, so the env
  // must be set (and the cache reset) before createAppStore.
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

  it('hides /sessions when the session-dashboard feature is off', () => {
    withFeatures('[]', () => {
      const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
      const visible = selectVisibleSlashCommands(store.getState());
      expect(visible.find((c) => c.name === '/sessions')).toBeUndefined();
    });
  });

  it('shows /sessions when the feature is on, in a local session', () => {
    withFeatures(JSON.stringify([Feature.SessionDashboard]), () => {
      const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
      const visible = selectVisibleSlashCommands(store.getState());
      expect(visible.find((c) => c.name === '/sessions')).toBeDefined();
    });
  });

  it('shows /sessions when the feature is on, in a cloud session', () => {
    withFeatures(JSON.stringify([Feature.SessionDashboard]), () => {
      const store = createAppStore({ kiro: new Kiro(), agentEngine: 'kas' });
      store.getState().setCloudSessionActive(true);
      const visible = selectVisibleSlashCommands(store.getState());
      expect(visible.find((c) => c.name === '/sessions')).toBeDefined();
    });
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
  // Workflow commands need the rollout AND the persisted opt-in, so the
  // preference is written to a scratch home alongside the feature list.
  const withFeatures = (value: string | undefined, fn: () => void) => {
    const prev = process.env.KIRO_ENABLED_FEATURES;
    const prevHome = process.env.KIRO_HOME;
    const home = mkdtempSync(join(tmpdir(), 'slash-visibility-'));
    mkdirSync(join(home, 'settings'), { recursive: true });
    writeFileSync(
      join(home, 'settings', 'cli.json'),
      JSON.stringify({
        'chat.enableWorkflows': (value ?? '').includes('workflows'),
      })
    );
    if (value === undefined) delete process.env.KIRO_ENABLED_FEATURES;
    else process.env.KIRO_ENABLED_FEATURES = value;
    process.env.KIRO_HOME = home;
    features._resetForTests();
    try {
      fn();
    } finally {
      if (prev === undefined) delete process.env.KIRO_ENABLED_FEATURES;
      else process.env.KIRO_ENABLED_FEATURES = prev;
      if (prevHome === undefined) delete process.env.KIRO_HOME;
      else process.env.KIRO_HOME = prevHome;
      features._resetForTests();
      rmSync(home, { recursive: true, force: true });
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
