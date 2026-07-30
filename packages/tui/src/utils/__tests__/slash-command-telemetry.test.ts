import { readFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { describe, expect, it } from 'bun:test';
import { KAS_COMMANDS } from '../../kas-commands.js';
import { Kiro } from '../../kiro.js';
import { createAppStore } from '../../stores/app-store.js';
import {
  SLASH_COMMAND_METRIC_NAMES,
  canonicalSlashCommandName,
  commandMetricName,
} from '../slash-command-telemetry.js';

const { load: loadYaml } = createRequire(import.meta.url)('js-yaml') as {
  load: (source: string) => unknown;
};

describe('slash-command telemetry names', () => {
  it('matches the slash-command values in the metric schema', () => {
    const schemaPath = new URL(
      '../../../../../crates/kiro-telemetry-schema/schema/types.yaml',
      import.meta.url
    );
    const schema = loadYaml(readFileSync(schemaPath, 'utf8')) as {
      attributes: Array<{ name: string; allowed_values?: string[] }>;
    };
    const schemaNames =
      schema.attributes
        .find((attribute) => attribute.name === 'command')
        ?.allowed_values?.filter((value) => value.startsWith('/'))
        .filter((value) => value !== '/custom') ?? [];

    expect(schemaNames).toEqual([...SLASH_COMMAND_METRIC_NAMES]);
  });

  it('accepts every static KAS command', () => {
    for (const command of KAS_COMMANDS) {
      expect(canonicalSlashCommandName(command.name)).toBe(command.name);
    }
  });

  it('accepts every static local command', () => {
    const commands = createAppStore({
      kiro: new Kiro(),
      agentEngine: 'v2',
    }).getState().slashCommands;
    for (const command of commands) {
      expect(canonicalSlashCommandName(command.name)).toBe(command.name);
    }
  });

  it('normalizes V1 names with or without a slash', () => {
    expect(canonicalSlashCommandName('help')).toBe('/help');
    expect(canonicalSlashCommandName('/HELP')).toBe('/help');
  });

  it('uses reviewed KAS telemetry metadata without accepting arbitrary names', () => {
    expect(
      commandMetricName({
        name: '/display-name',
        description: '',
        meta: { telemetryId: 'workflow-run' },
      })
    ).toBe('/workflow-run');
    expect(
      commandMetricName({
        name: '/display-name',
        description: '',
        meta: { telemetryId: 'unreviewed-server-command' },
      })
    ).toBe('/custom');
  });

  it('buckets dynamic and unrecognized commands', () => {
    expect(
      commandMetricName({
        name: '/my-prompt',
        description: '',
        meta: { type: 'prompt' },
      })
    ).toBe('/prompt');
    expect(
      commandMetricName({
        name: '/quick-spec',
        description: '',
        meta: { type: 'steering' },
      })
    ).toBe('/steering');
    expect(canonicalSlashCommandName('/user-command')).toBe('/custom');
  });
});
