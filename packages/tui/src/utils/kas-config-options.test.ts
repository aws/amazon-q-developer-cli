import { describe, it, expect } from 'bun:test';
import {
  parseModelsFromConfigOptions,
  parseEffortsFromConfigOptions,
  parseAgentsFromConfigOptions,
  toKasModeId,
  fromKasModeId,
  resolveInitialModel,
  resolveEffortToApply,
  shouldApplyEffortDefault,
} from './kas-config-options';

describe('mode id mapping', () => {
  it('maps TUI ids to KAS wire ids', () => {
    expect(toKasModeId('kiro_planner')).toBe('plan');
    expect(toKasModeId('default')).toBe('vibe');
    expect(toKasModeId('spec')).toBe('spec');
  });

  it('maps KAS wire ids back to TUI ids', () => {
    expect(fromKasModeId('plan')).toBe('kiro_planner');
    expect(fromKasModeId('vibe')).toBe('default');
    expect(fromKasModeId('spec')).toBe('spec');
  });

  it('passes the autonomous mode id through unchanged in both directions', () => {
    expect(fromKasModeId('autonomous')).toBe('autonomous');
    expect(toKasModeId('autonomous')).toBe('autonomous');
  });
});

describe('parseModelsFromConfigOptions', () => {
  it('returns undefined when no model category is present', () => {
    expect(parseModelsFromConfigOptions([])).toBeUndefined();
    expect(parseModelsFromConfigOptions(undefined)).toBeUndefined();
    expect(
      parseModelsFromConfigOptions([{ id: 'effortLevel', type: 'select' }])
    ).toBeUndefined();
  });

  it('extracts models, current id, and rate metadata', () => {
    const configOptions = [
      {
        id: 'model',
        category: 'model',
        type: 'select',
        currentValue: 'claude-sonnet-4',
        options: [
          {
            value: 'claude-sonnet-4',
            name: 'Claude Sonnet 4',
            description: 'balanced',
            _meta: { kiro: { rateMultiplier: 1, rateUnit: 'credits' } },
          },
          {
            value: 'claude-opus-4',
            name: 'Claude Opus 4',
            _meta: { kiro: { rateMultiplier: 4 } },
          },
        ],
      },
    ];
    const result = parseModelsFromConfigOptions(configOptions);
    expect(result?.currentModelId).toBe('claude-sonnet-4');
    expect(result?.models).toEqual([
      {
        id: 'claude-sonnet-4',
        name: 'Claude Sonnet 4',
        description: 'balanced',
        rateMultiplier: 1,
        rateUnit: 'credits',
      },
      {
        id: 'claude-opus-4',
        name: 'Claude Opus 4',
        description: undefined,
        rateMultiplier: 4,
        rateUnit: undefined,
      },
    ]);
  });

  it('drops malformed option entries', () => {
    const result = parseModelsFromConfigOptions([
      {
        category: 'model',
        type: 'select',
        currentValue: 'a',
        options: [
          { value: 'a', name: 'A' },
          { value: 123, name: 'bad' },
          { value: 'c' },
        ],
      },
    ]);
    expect(result?.models.map((m) => m.id)).toEqual(['a']);
  });
});

describe('parseEffortsFromConfigOptions', () => {
  it('returns undefined when no effortLevel entry is present', () => {
    expect(parseEffortsFromConfigOptions([])).toBeUndefined();
    expect(
      parseEffortsFromConfigOptions([{ category: 'model', type: 'select' }])
    ).toBeUndefined();
  });

  it('extracts levels and current level', () => {
    const result = parseEffortsFromConfigOptions([
      {
        id: 'effortLevel',
        type: 'select',
        currentValue: 'high',
        options: [
          { value: 'low', name: 'Low' },
          { value: 'high', name: 'High' },
        ],
      },
    ]);
    expect(result?.currentLevel).toBe('high');
    expect(result?.efforts).toEqual([
      { value: 'low', name: 'Low' },
      { value: 'high', name: 'High' },
    ]);
  });

  it('reports null current level when currentValue is absent', () => {
    const result = parseEffortsFromConfigOptions([
      {
        id: 'effortLevel',
        type: 'select',
        options: [{ value: 'low', name: 'Low' }],
      },
    ]);
    expect(result?.currentLevel).toBeNull();
  });
});

describe('parseAgentsFromConfigOptions', () => {
  it('returns undefined when no mode entry is present', () => {
    expect(parseAgentsFromConfigOptions([])).toBeUndefined();
    expect(
      parseAgentsFromConfigOptions([{ category: 'model', type: 'select' }])
    ).toBeUndefined();
  });

  it('normalizes ids, extracts source + welcomeMessage, and current id', () => {
    const result = parseAgentsFromConfigOptions([
      {
        id: 'mode',
        category: 'mode',
        type: 'select',
        currentValue: 'plan',
        options: [
          {
            value: 'vibe',
            name: 'Default',
            description: 'general',
            _meta: { kiro: { source: 'bundled' } },
          },
          {
            value: 'plan',
            name: 'Planner',
            _meta: { kiro: { source: 'bundled', welcomeMessage: 'Hi!' } },
          },
          {
            value: 'greeter',
            name: 'Greeter',
            _meta: { kiro: { source: 'workspace', welcomeMessage: 'build?' } },
          },
        ],
      },
    ]);
    expect(result?.currentAgentId).toBe('kiro_planner');
    expect(result?.agents).toEqual([
      {
        id: 'default',
        name: 'Default',
        description: 'general',
        source: 'bundled',
        welcomeMessage: undefined,
      },
      {
        id: 'kiro_planner',
        name: 'Planner',
        description: undefined,
        source: 'bundled',
        welcomeMessage: 'Hi!',
      },
      {
        id: 'greeter',
        name: 'Greeter',
        description: undefined,
        source: 'workspace',
        welcomeMessage: 'build?',
      },
    ]);
  });

  it('hides bundled agents not on the allowlist but keeps user/workspace agents', () => {
    const result = parseAgentsFromConfigOptions([
      {
        id: 'mode',
        category: 'mode',
        type: 'select',
        currentValue: 'vibe',
        options: [
          {
            value: 'vibe',
            name: 'Default',
            _meta: { kiro: { source: 'bundled' } },
          },
          {
            value: 'semantic_reviewer',
            name: 'Reviewer',
            _meta: { kiro: { source: 'bundled' } },
          },
          {
            value: 'my_agent',
            name: 'Mine',
            _meta: { kiro: { source: 'workspace' } },
          },
          { value: 'no_source_agent', name: 'NoSource' },
        ],
      },
    ]);
    expect(result?.agents.map((a) => a.id)).toEqual([
      'default',
      'my_agent',
      'no_source_agent',
    ]);
  });

  it('falls back to top-level _meta.welcomeMessage for older KAS', () => {
    const result = parseAgentsFromConfigOptions([
      {
        id: 'mode',
        category: 'mode',
        type: 'select',
        currentValue: 'greeter',
        options: [
          {
            value: 'greeter',
            name: 'Greeter',
            _meta: { welcomeMessage: 'legacy', kiro: { source: 'workspace' } },
          },
        ],
      },
    ]);
    expect(result?.agents[0]?.welcomeMessage).toBe('legacy');
  });
});

describe('resolveInitialModel', () => {
  it('prefers an explicit --model flag over a saved default', () => {
    expect(
      resolveInitialModel({
        flagModel: 'gpt-5.1',
        savedDefaultModel: 'claude-opus-4.8',
      })
    ).toBe('gpt-5.1');
  });

  it('falls back to the saved default when no flag is given', () => {
    expect(resolveInitialModel({ savedDefaultModel: 'claude-opus-4.8' })).toBe(
      'claude-opus-4.8'
    );
    expect(
      resolveInitialModel({
        flagModel: null,
        savedDefaultModel: 'claude-opus-4.8',
      })
    ).toBe('claude-opus-4.8');
  });

  it('returns null when neither a flag nor a saved default is present', () => {
    expect(resolveInitialModel({})).toBeNull();
    expect(
      resolveInitialModel({ flagModel: null, savedDefaultModel: null })
    ).toBeNull();
  });
});

describe('resolveEffortToApply', () => {
  const base = {
    currentModelId: 'claude-opus-4.8',
    availableEfforts: ['low', 'medium', 'high', 'xhigh'],
    currentEffort: 'high',
    savedEffortForModel: 'medium',
    shouldApply: true,
  };

  it('applies the saved effort when eligible, valid, and different from current', () => {
    expect(resolveEffortToApply(base)).toBe('medium');
  });

  it('returns null when shouldApply is false (origin/session gate not met)', () => {
    expect(resolveEffortToApply({ ...base, shouldApply: false })).toBeNull();
  });

  it('returns null when there is no current model', () => {
    expect(resolveEffortToApply({ ...base, currentModelId: null })).toBeNull();
    expect(
      resolveEffortToApply({ ...base, currentModelId: undefined })
    ).toBeNull();
  });

  it('returns null when the model has no saved default', () => {
    expect(
      resolveEffortToApply({ ...base, savedEffortForModel: null })
    ).toBeNull();
  });

  it('returns null when the saved default is not an available level', () => {
    expect(
      resolveEffortToApply({ ...base, savedEffortForModel: 'ultra' })
    ).toBeNull();
  });

  it('returns null when the saved default already equals the current level', () => {
    expect(
      resolveEffortToApply({ ...base, currentEffort: 'medium' })
    ).toBeNull();
  });
});

describe('parseModelsFromConfigOptions effortSchemaPath', () => {
  it('surfaces the advertised effortSchemaPath and ignores missing/invalid values', () => {
    const parsed = parseModelsFromConfigOptions([
      {
        id: 'model',
        category: 'model',
        type: 'select',
        currentValue: 'a',
        options: [
          {
            value: 'a',
            name: 'A',
            _meta: { kiro: { effortSchemaPath: 'output_config' } },
          },
          {
            value: 'b',
            name: 'B',
            _meta: { kiro: { effortSchemaPath: 'reasoning' } },
          },
          { value: 'c', name: 'C', _meta: { kiro: { hasEffort: true } } },
          {
            value: 'd',
            name: 'D',
            _meta: { kiro: { effortSchemaPath: 'bogus' } },
          },
          { value: 'e', name: 'E' },
        ],
      },
    ]);
    const byId = Object.fromEntries(
      (parsed?.models ?? []).map((m) => [m.id, m.effortSchemaPath])
    );
    expect(byId).toEqual({
      a: 'output_config',
      b: 'reasoning',
      c: undefined,
      d: undefined,
      e: undefined,
    });
  });
});

describe('shouldApplyEffortDefault', () => {
  it('applies in a new session when the model changed', () => {
    expect(
      shouldApplyEffortDefault({
        origin: 'newSession',
        sessionOrigin: 'new',
        modelChanged: true,
        hasExplicitEffort: false,
        hadPriorModel: false,
      })
    ).toBe(true);
  });

  it('applies on the first model resolution in a new session (serverPush, no prior model)', () => {
    expect(
      shouldApplyEffortDefault({
        origin: 'serverPush',
        sessionOrigin: 'new',
        modelChanged: true,
        hasExplicitEffort: false,
        hadPriorModel: false,
      })
    ).toBe(true);
  });

  it('does NOT apply on a later autonomous push in a new session (fallback never stomps effort)', () => {
    expect(
      shouldApplyEffortDefault({
        origin: 'serverPush',
        sessionOrigin: 'new',
        modelChanged: true,
        hasExplicitEffort: false,
        hadPriorModel: true,
      })
    ).toBe(false);
  });

  it('does NOT apply in a new session launched with --effort (explicit flag wins)', () => {
    expect(
      shouldApplyEffortDefault({
        origin: 'newSession',
        sessionOrigin: 'new',
        modelChanged: true,
        hasExplicitEffort: true,
        hadPriorModel: false,
      })
    ).toBe(false);
  });

  it('does NOT apply on a resumed session (keeps its own effort)', () => {
    expect(
      shouldApplyEffortDefault({
        origin: 'loadSession',
        sessionOrigin: 'resumed',
        modelChanged: true,
        hasExplicitEffort: false,
        hadPriorModel: false,
      })
    ).toBe(false);
  });

  it('applies on an explicit client switch, even in a resumed session', () => {
    expect(
      shouldApplyEffortDefault({
        origin: 'clientInitiated',
        sessionOrigin: 'resumed',
        modelChanged: true,
        hasExplicitEffort: false,
        hadPriorModel: true,
      })
    ).toBe(true);
  });

  it('applies on an explicit client switch even with a prior model and --effort (flag only wins at launch)', () => {
    expect(
      shouldApplyEffortDefault({
        origin: 'clientInitiated',
        sessionOrigin: 'new',
        modelChanged: true,
        hasExplicitEffort: true,
        hadPriorModel: true,
      })
    ).toBe(true);
  });

  it('does NOT apply on an autonomous push in a resumed session (v2 parity)', () => {
    expect(
      shouldApplyEffortDefault({
        origin: 'serverPush',
        sessionOrigin: 'resumed',
        modelChanged: true,
        hasExplicitEffort: false,
        hadPriorModel: true,
      })
    ).toBe(false);
  });

  it('does NOT apply when the model did not change (effort echo / no-loop)', () => {
    for (const origin of [
      'clientInitiated',
      'serverPush',
      'newSession',
    ] as const) {
      expect(
        shouldApplyEffortDefault({
          origin,
          sessionOrigin: 'new',
          modelChanged: false,
          hasExplicitEffort: false,
          hadPriorModel: false,
        })
      ).toBe(false);
    }
  });
});
