import { describe, it, expect, mock, beforeEach, afterEach } from 'bun:test';
import { mkdirSync, writeFileSync, readFileSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import { handleModel } from '../model';
import { handleAgent, createAgent, editAgent } from '../agent';
import { handleEffort } from '../effort';
import { createMockCommandContext } from '../../__tests__/test-helpers';
import type { KasCommand } from '../../../kas-commands';
import { KasCommandName } from '../../../kas-commands';

// Drive cli.json persistence through a temp HOME rather than mock.module:
// module mocks leak across files in bun's shared test process. Mirrors the
// convention documented in kas-acp-client.test.ts.
let testHome: string;
let originalHome: string | undefined;

function cliJsonPath() {
  return join(testHome, '.kiro', 'settings', 'cli.json');
}
function readCliJson(): Record<string, unknown> {
  return JSON.parse(readFileSync(cliJsonPath(), 'utf-8'));
}

beforeEach(() => {
  testHome = join(
    tmpdir(),
    `kas-handler-test-${Date.now()}-${Math.random().toString(36).slice(2)}`
  );
  mkdirSync(join(testHome, '.kiro', 'settings'), { recursive: true });
  writeFileSync(cliJsonPath(), '{}', 'utf-8');
  originalHome = process.env.HOME;
  process.env.HOME = testHome;
});

afterEach(() => {
  if (originalHome !== undefined) process.env.HOME = originalHome;
  else delete process.env.HOME;
});

const MODEL_CMD: KasCommand = {
  name: KasCommandName.Model,
  description: '',
  meta: { inputType: 'selection' },
};
const AGENT_CMD: KasCommand = {
  name: KasCommandName.Agent,
  description: '',
  meta: { inputType: 'selection' },
};
const EFFORT_CMD: KasCommand = {
  name: KasCommandName.Effort,
  description: '',
  meta: { inputType: 'selection' },
};

describe('handleModel', () => {
  it('builds the picker from kasAvailableModels with active + credits', async () => {
    const ctx = createMockCommandContext({
      kasAvailableModels: [
        { id: 'sonnet', name: 'Sonnet', rateMultiplier: 1 },
        { id: 'opus', name: 'Opus', description: 'big', rateMultiplier: 4 },
      ],
      currentModel: { id: 'sonnet', name: 'Sonnet' },
    });
    await handleModel(MODEL_CMD, '', ctx);
    const call = (ctx._spies.setActiveCommand as any).mock.calls[0][0];
    expect(call.options).toEqual([
      {
        value: 'sonnet',
        label: 'Sonnet',
        description: '[active]',
        group: '1.00x credits',
      },
      {
        value: 'opus',
        label: 'Opus',
        description: 'big',
        group: '4.00x credits',
      },
    ]);
  });

  it('alerts when no models available', async () => {
    const ctx = createMockCommandContext({ kasAvailableModels: [] });
    await handleModel(MODEL_CMD, '', ctx);
    expect(ctx._spies.setActiveCommand).not.toHaveBeenCalled();
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'No models available',
      'error',
      3000
    );
  });

  it('shows a waiting warning (not an error) for an empty list on a cloud session', async () => {
    const ctx = createMockCommandContext({ kasAvailableModels: [] });
    ctx.cloudSessionActive = true;
    await handleModel(MODEL_CMD, '', ctx);
    expect(ctx._spies.setActiveCommand).not.toHaveBeenCalled();
    const [message, status] = (ctx._spies.showAlert as any).mock.calls[0];
    expect(String(message)).toContain('sandbox');
    expect(status).toBe('warning');
  });

  it('switches via setConfigOption without persisting a default', async () => {
    const setConfigOption = mock(() => Promise.resolve());
    const ctx = createMockCommandContext({
      kasAvailableModels: [{ id: 'opus', name: 'Opus' }],
      currentModel: { id: 'opus', name: 'Opus' },
      kiro: { setConfigOption } as any,
    });
    await handleModel(MODEL_CMD, 'opus', ctx);
    expect(setConfigOption).toHaveBeenCalledWith('model', 'opus');
    expect(readCliJson()['chat.defaultModel']).toBeUndefined();
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'Switched to Opus',
      'success',
      3000
    );
  });

  it('errors when the switch does not land', async () => {
    const ctx = createMockCommandContext({
      kasAvailableModels: [{ id: 'opus', name: 'Opus' }],
      currentModel: { id: 'sonnet', name: 'Sonnet' },
      kiro: { setConfigOption: mock(() => Promise.resolve()) } as any,
    });
    await handleModel(MODEL_CMD, 'opus', ctx);
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      "Model 'opus' not available",
      'error',
      5000
    );
  });
});

describe('handleEffort', () => {
  it('alerts when effort unavailable', async () => {
    const ctx = createMockCommandContext({ kasAvailableEfforts: [] });
    await handleEffort(EFFORT_CMD, '', ctx);
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'Effort is not available on the current model. Select a model that supports effort levels.',
      'error',
      5000
    );
  });

  it('marks the current level as [active] and the model default as [default]', async () => {
    const ctx = createMockCommandContext({
      kasAvailableEfforts: [
        { value: 'low', name: 'Low' },
        { value: 'medium', name: 'Medium' },
        { value: 'high', name: 'High' },
      ],
      kasAvailableModels: [
        { id: 'opus', name: 'Opus', defaultEffortLevel: 'medium' },
      ],
      currentModel: { id: 'opus', name: 'Opus' },
      currentEffort: 'high',
    });
    await handleEffort(EFFORT_CMD, '', ctx);
    const call = (ctx._spies.setActiveCommand as any).mock.calls[0][0];
    expect(call.options).toEqual([
      { value: 'low', label: 'low', description: '' },
      { value: 'medium', label: 'medium', description: '[default]' },
      { value: 'high', label: 'high', description: '[active]' },
    ]);
  });

  it('[active] takes precedence over [default] when current equals model default', async () => {
    const ctx = createMockCommandContext({
      kasAvailableEfforts: [
        { value: 'low', name: 'Low' },
        { value: 'high', name: 'High' },
      ],
      kasAvailableModels: [
        { id: 'opus', name: 'Opus', defaultEffortLevel: 'high' },
      ],
      currentModel: { id: 'opus', name: 'Opus' },
      currentEffort: 'high',
    });
    await handleEffort(EFFORT_CMD, '', ctx);
    const call = (ctx._spies.setActiveCommand as any).mock.calls[0][0];
    expect(call.options).toEqual([
      { value: 'low', label: 'low', description: '' },
      { value: 'high', label: 'high', description: '[active]' },
    ]);
  });

  it('shows no [default] when model has no defaultEffortLevel', async () => {
    const ctx = createMockCommandContext({
      kasAvailableEfforts: [
        { value: 'low', name: 'Low' },
        { value: 'high', name: 'High' },
      ],
      kasAvailableModels: [{ id: 'opus', name: 'Opus' }],
      currentModel: { id: 'opus', name: 'Opus' },
      currentEffort: 'high',
    });
    await handleEffort(EFFORT_CMD, '', ctx);
    const call = (ctx._spies.setActiveCommand as any).mock.calls[0][0];
    expect(call.options).toEqual([
      { value: 'low', label: 'low', description: '' },
      { value: 'high', label: 'high', description: '[active]' },
    ]);
  });

  it('switches and confirms with lowercased label without persisting', async () => {
    const setConfigOption = mock(() => Promise.resolve());
    const ctx = createMockCommandContext({
      kasAvailableEfforts: [{ value: 'xhigh', name: 'xHigh' }],
      kasAvailableModels: [
        { id: 'opus', name: 'Opus', effortSchemaPath: 'output_config' },
      ],
      currentModel: { id: 'opus', name: 'Opus' },
      currentEffort: 'xhigh',
      kiro: { setConfigOption } as any,
    });
    await handleEffort(EFFORT_CMD, 'xhigh', ctx);
    expect(setConfigOption).toHaveBeenCalledWith('effortLevel', 'xhigh');
    expect(readCliJson()['chat.modelDefaults']).toBeUndefined();
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'Effort set to xhigh',
      'success',
      3000
    );
  });
});

describe('handleAgent', () => {
  it('builds the picker grouped by source', async () => {
    const ctx = createMockCommandContext({
      kasAvailableAgents: [
        { id: 'default', name: 'Default', source: 'bundled' },
        { id: 'mine', name: 'Mine', description: 'd', source: 'workspace' },
      ],
      currentAgent: { name: 'default' },
    });
    await handleAgent(AGENT_CMD, '', ctx);
    const call = (ctx._spies.setActiveCommand as any).mock.calls[0][0];
    expect(call.options[0]).toMatchObject({
      value: 'default',
      description: '[active]',
      group: 'Bundled',
    });
    expect(call.options[1]).toMatchObject({
      value: 'mine',
      description: 'd',
      group: 'Workspace',
    });
  });

  it('swaps via setConfigOption(mode, …) and confirms', async () => {
    const setConfigOption = mock(() => Promise.resolve());
    const ctx = createMockCommandContext({
      kasAvailableAgents: [{ id: 'kiro_planner', name: 'Planner' }],
      currentAgent: { name: 'kiro_planner' },
      kiro: { setConfigOption } as any,
    });
    await handleAgent(AGENT_CMD, 'kiro_planner', ctx);
    expect(setConfigOption).toHaveBeenCalledWith('mode', 'kiro_planner');
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'Switched to kiro_planner',
      'success',
      3000
    );
  });
});

describe('handleAgent create', () => {
  // The real editor launch (spawn + TTY handoff) is bypassed through the
  // injectable OpenEditor seam; routing-level cases go through handleAgent.
  let originalKiroHome: string | undefined;

  beforeEach(() => {
    originalKiroHome = process.env.KIRO_HOME;
    delete process.env.KIRO_HOME; // the temp $HOME must be authoritative
  });

  afterEach(() => {
    if (originalKiroHome !== undefined)
      process.env.KIRO_HOME = originalKiroHome;
  });

  const editorOk = () => ({ exitCode: 0 });

  function agentsDir() {
    return join(testHome, '.kiro', 'agents');
  }

  function createParsed(
    overrides: Partial<{
      name: string;
      from: string;
      directory: string;
    }> = {}
  ) {
    return {
      kind: 'create' as const,
      name: undefined,
      from: undefined,
      directory: undefined,
      ...overrides,
    };
  }

  it('requires a name (routed via handleAgent)', async () => {
    const ctx = createMockCommandContext();
    await handleAgent(AGENT_CMD, 'create', ctx);
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'Agent name is required. Usage: /agent create <name> [--from <agent>] [--directory <path>]',
      'error',
      5000
    );
  });

  it('scaffolds a profile in the global agents dir and confirms', async () => {
    const ctx = createMockCommandContext();
    await createAgent(ctx, createParsed({ name: 'my-agent' }), editorOk);
    const filePath = join(agentsDir(), 'my-agent.json');
    const written = JSON.parse(readFileSync(filePath, 'utf-8'));
    expect(written).toEqual({
      name: 'my-agent',
      description: '',
      prompt: '',
      tools: [],
    });
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      `Agent 'my-agent' created at ${filePath}`,
      'success',
      5000
    );
  });

  it('honors --directory with a custom path', async () => {
    const dir = join(testHome, 'custom agents');
    const ctx = createMockCommandContext();
    await createAgent(
      ctx,
      createParsed({ name: 'custom', directory: dir }),
      editorOk
    );
    expect(
      JSON.parse(readFileSync(join(dir, 'custom.json'), 'utf-8')).name
    ).toBe('custom');
  });

  it('aborts when the profile file already exists', async () => {
    mkdirSync(agentsDir(), { recursive: true });
    const existing = join(agentsDir(), 'dupe.json');
    writeFileSync(existing, '{}', 'utf-8');
    const ctx = createMockCommandContext();
    await createAgent(ctx, createParsed({ name: 'dupe' }), editorOk);
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      `File already exists at ${existing}. Aborting`,
      'error',
      5000
    );
    expect(readFileSync(existing, 'utf-8')).toBe('{}');
  });

  it('copies an existing profile with --from, renaming it', async () => {
    mkdirSync(agentsDir(), { recursive: true });
    writeFileSync(
      join(agentsDir(), 'base.json'),
      JSON.stringify({ name: 'base', description: 'base agent', tools: '*' }),
      'utf-8'
    );
    const ctx = createMockCommandContext();
    await createAgent(
      ctx,
      createParsed({ name: 'copy', from: 'base' }),
      editorOk
    );
    const written = JSON.parse(
      readFileSync(join(agentsDir(), 'copy.json'), 'utf-8')
    );
    expect(written).toEqual({
      name: 'copy',
      description: 'base agent',
      tools: '*',
    });
  });

  it('copies a markdown profile with --from, rewriting its name pin', async () => {
    mkdirSync(agentsDir(), { recursive: true });
    writeFileSync(
      join(agentsDir(), 'base.md'),
      '---\nname: base\ndescription: reviews code\n---\n\nPrompt body\n',
      'utf-8'
    );
    const ctx = createMockCommandContext();
    await createAgent(
      ctx,
      createParsed({ name: 'copy', from: 'base' }),
      editorOk
    );
    const written = readFileSync(join(agentsDir(), 'copy.md'), 'utf-8');
    expect(written).toContain('name: copy');
    expect(written).toContain('Prompt body');
    expect(written).not.toContain('name: base');
  });

  it('errors when the --from agent does not exist', async () => {
    const ctx = createMockCommandContext();
    await createAgent(
      ctx,
      createParsed({ name: 'copy', from: 'missing-agent' }),
      editorOk
    );
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      "No agent with name 'missing-agent' found",
      'error',
      5000
    );
  });

  it('surfaces a failed editor without a success alert', async () => {
    const ctx = createMockCommandContext();
    await createAgent(ctx, createParsed({ name: 'ed-fail' }), () => ({
      exitCode: 1,
    }));
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'Editor exited with code 1',
      'error',
      3000
    );
  });

  it('reports malformed JSON after editing', async () => {
    const ctx = createMockCommandContext();
    const corruptingEditor = (filePath: string) => {
      writeFileSync(filePath, 'not-json', 'utf-8');
      return { exitCode: 0 };
    };
    await createAgent(
      ctx,
      createParsed({ name: 'bad-json' }),
      corruptingEditor
    );
    const [message, status] = (ctx._spies.showAlert as any).mock.calls.at(-1);
    expect(status).toBe('error');
    expect(message).toContain('Malformed agent config at');
  });

  it('rejects an emptied name field after editing', async () => {
    const ctx = createMockCommandContext();
    const blankingEditor = (filePath: string) => {
      writeFileSync(filePath, JSON.stringify({ name: ' ' }), 'utf-8');
      return { exitCode: 0 };
    };
    await createAgent(ctx, createParsed({ name: 'blank' }), blankingEditor);
    const [message, status] = (ctx._spies.showAlert as any).mock.calls.at(-1);
    expect(status).toBe('error');
    expect(message).toContain('"name" must be a non-empty string');
  });

  it('is blocked in cloud sessions', async () => {
    const ctx = createMockCommandContext({ cloudSessionActive: true });
    await handleAgent(AGENT_CMD, 'create cloudy', ctx);
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      '/agent create is not available in cloud sessions',
      'error',
      5000
    );
  });
});

describe('handleAgent edit', () => {
  let originalKiroHome: string | undefined;

  beforeEach(() => {
    originalKiroHome = process.env.KIRO_HOME;
    delete process.env.KIRO_HOME; // the temp $HOME must be authoritative
  });

  afterEach(() => {
    if (originalKiroHome !== undefined)
      process.env.KIRO_HOME = originalKiroHome;
  });

  const editorOk = () => ({ exitCode: 0 });

  function agentsDir() {
    return join(testHome, '.kiro', 'agents');
  }

  function writeProfile(name: string) {
    mkdirSync(agentsDir(), { recursive: true });
    const filePath = join(agentsDir(), `${name}.json`);
    writeFileSync(filePath, JSON.stringify({ name }), 'utf-8');
    return filePath;
  }

  it('opens the profile of the named agent and confirms', async () => {
    const filePath = writeProfile('my-agent');
    const opened: string[] = [];
    const ctx = createMockCommandContext();
    await editAgent(ctx, 'my-agent', (p) => {
      opened.push(p);
      return { exitCode: 0 };
    });
    expect(opened).toEqual([filePath]);
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      `Edited agent 'my-agent' at ${filePath}`,
      'success',
      5000
    );
  });

  it('defaults to the active agent when no name is given', async () => {
    const filePath = writeProfile('current-agent');
    const ctx = createMockCommandContext({
      currentAgent: { name: 'current-agent' },
    });
    await editAgent(ctx, undefined, editorOk);
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      `Edited agent 'current-agent' at ${filePath}`,
      'success',
      5000
    );
  });

  it('rejects built-in agents', async () => {
    const ctx = createMockCommandContext({
      kasAvailableAgents: [{ id: 'vibe', name: 'Vibe', source: 'bundled' }],
    });
    await editAgent(ctx, 'vibe', editorOk);
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      "Cannot edit built-in agent 'vibe'. Create a new agent with '/agent create'",
      'error',
      5000
    );
  });

  it('errors when the agent does not exist', async () => {
    const ctx = createMockCommandContext();
    await editAgent(ctx, 'missing-agent', editorOk);
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      "Agent 'missing-agent' not found",
      'error',
      5000
    );
  });

  it('distinguishes a listed agent with no file on disk', async () => {
    const ctx = createMockCommandContext({
      kasAvailableAgents: [{ id: 'ghost', name: 'Ghost', source: 'workspace' }],
    });
    await editAgent(ctx, 'ghost', editorOk);
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      "Agent 'ghost' has no config file on disk",
      'error',
      5000
    );
  });

  it('reports malformed JSON after editing', async () => {
    writeProfile('bad-edit');
    const ctx = createMockCommandContext();
    const corruptingEditor = (filePath: string) => {
      writeFileSync(filePath, '{oops', 'utf-8');
      return { exitCode: 0 };
    };
    await editAgent(ctx, 'bad-edit', corruptingEditor);
    const [message, status] = (ctx._spies.showAlert as any).mock.calls.at(-1);
    expect(status).toBe('error');
    expect(message).toContain('Malformed agent config at');
  });

  it('surfaces a failed editor without a success alert', async () => {
    writeProfile('ed-fail');
    const ctx = createMockCommandContext();
    await editAgent(ctx, 'ed-fail', () => ({ exitCode: 1 }));
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'Editor exited with code 1',
      'error',
      3000
    );
  });

  it('is blocked in cloud sessions', async () => {
    const ctx = createMockCommandContext({ cloudSessionActive: true });
    await handleAgent(AGENT_CMD, 'edit foo', ctx);
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      '/agent edit is not available in cloud sessions',
      'error',
      5000
    );
  });
});

describe('set-current-as-default persistence', () => {
  it('/model set-current-as-default persists the active model to cli.json', async () => {
    const ctx = createMockCommandContext({
      currentModel: { id: 'opus', name: 'Opus' },
    });
    await handleModel(MODEL_CMD, 'set-current-as-default', ctx);
    expect(readCliJson()['chat.defaultModel']).toBe('opus');
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'Saved Opus as default model',
      'success',
      3000
    );
  });

  it('/model set-current-as-default errors when no model is active', async () => {
    const ctx = createMockCommandContext({ currentModel: null });
    await handleModel(MODEL_CMD, 'set-current-as-default', ctx);
    expect(readCliJson()['chat.defaultModel']).toBeUndefined();
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'Select a model to save as the default',
      'error',
      3000
    );
  });

  it('/model set-current-as-default reports an error when persistence fails', async () => {
    // Persisting IS the command here, so a failed write must surface as an
    // error - but gracefully, not as an unhandled throw.
    writeFileSync(cliJsonPath(), 'not json', 'utf-8');
    const ctx = createMockCommandContext({
      currentModel: { id: 'opus', name: 'Opus' },
    });
    await handleModel(MODEL_CMD, 'set-current-as-default', ctx);
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'Failed to save Opus as default model',
      'error',
      5000
    );
  });

  it('/effort set-current-as-default persists at the advertised schema path', async () => {
    const ctx = createMockCommandContext({
      kasAvailableEfforts: [{ value: 'high', name: 'High' }],
      kasAvailableModels: [
        { id: 'opus', name: 'Opus', effortSchemaPath: 'output_config' },
      ],
      currentModel: { id: 'opus', name: 'Opus' },
      currentEffort: 'high',
    });
    await handleEffort(EFFORT_CMD, 'set-current-as-default', ctx);
    const defaults = readCliJson()['chat.modelDefaults'] as
      | Record<string, any>
      | undefined;
    expect(defaults?.opus?.output_config?.effort).toBe('high');
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'Set high as default effort for Opus',
      'success',
      3000
    );
  });

  it('/effort set-current-as-default errors without writing when no schema path is advertised', async () => {
    const ctx = createMockCommandContext({
      kasAvailableEfforts: [{ value: 'low', name: 'Low' }],
      kasAvailableModels: [{ id: 'gpt-5.1', name: 'GPT 5.1' }],
      currentModel: { id: 'gpt-5.1', name: 'GPT 5.1' },
      currentEffort: 'low',
    });
    await handleEffort(EFFORT_CMD, 'set-current-as-default', ctx);
    expect(readCliJson()['chat.modelDefaults']).toBeUndefined();
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'Effort defaults are not available for GPT 5.1',
      'error',
      5000
    );
  });

  it('/effort set-current-as-default errors when no model is active', async () => {
    const ctx = createMockCommandContext({
      currentModel: null,
      currentEffort: 'high',
    });
    await handleEffort(EFFORT_CMD, 'set-current-as-default', ctx);
    expect(readCliJson()['chat.modelDefaults']).toBeUndefined();
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'Select a model with an effort configured to save as the default',
      'error',
      3000
    );
  });

  it('/effort set-current-as-default errors when no effort level is set', async () => {
    const ctx = createMockCommandContext({
      currentModel: { id: 'opus', name: 'Opus' },
      currentEffort: null,
    });
    await handleEffort(EFFORT_CMD, 'set-current-as-default', ctx);
    expect(readCliJson()['chat.modelDefaults']).toBeUndefined();
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'No effort level is currently set. Effort may not be available on Opus.',
      'error',
      3000
    );
  });

  it('/effort set-current-as-default reports an error when persistence fails', async () => {
    writeFileSync(cliJsonPath(), 'not json', 'utf-8');
    const ctx = createMockCommandContext({
      kasAvailableModels: [
        { id: 'opus', name: 'Opus', effortSchemaPath: 'output_config' },
      ],
      currentModel: { id: 'opus', name: 'Opus' },
      currentEffort: 'high',
    });
    await handleEffort(EFFORT_CMD, 'set-current-as-default', ctx);
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      'Failed to save high as default effort for Opus',
      'error',
      5000
    );
  });
});
