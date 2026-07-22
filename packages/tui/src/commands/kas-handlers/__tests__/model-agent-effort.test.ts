import { describe, it, expect, mock, beforeEach, afterEach } from 'bun:test';
import { mkdirSync, writeFileSync, readFileSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import { handleModel } from '../model';
import { handleAgent } from '../agent';
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

  it('switches and confirms with display-cased label without persisting', async () => {
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
      'Effort set to xHigh',
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

  it('reports create/edit as not implemented', async () => {
    const ctx = createMockCommandContext();
    await handleAgent(AGENT_CMD, 'create foo', ctx);
    expect(ctx._spies.showAlert).toHaveBeenCalledWith(
      '/agent create is not yet implemented in KAS mode',
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
      'Set High as default effort for Opus',
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
      'Failed to save High as default effort for Opus',
      'error',
      5000
    );
  });
});
