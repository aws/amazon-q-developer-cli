/**
 * Unit tests for the `/settings <sub>` registry (`settings-subcommands.ts`).
 *
 * This is the typed/CLI-arg surface (`/settings terminal:interrupt`), parallel
 * to the visual SettingsPanel. The interrupt-behaviour option must stay
 * registered here and must persist `chat.defaultInterruptBehavior` — a
 * regression guard for the option going missing or stopping writing settings.
 */

import { describe, it, expect, beforeEach, afterEach } from 'bun:test';
import { mkdirSync, rmSync, readFileSync, existsSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import {
  settingsSubcommands,
  findSettingsSubcommand,
  type SettingsHandleContext,
} from '../settings-subcommands.js';
import { createMockCommandContext } from './test-helpers.js';
import type { SlashCommand } from '../../stores/app-store.js';

const settingsCmd: SlashCommand = {
  name: '/settings',
  description: 'Configure preferences',
  source: 'local',
  meta: { local: true },
};

/** Build a SettingsHandleContext around the shared mock CommandContext. */
function makeHandleCtx(): {
  handleCtx: SettingsHandleContext;
  ctx: ReturnType<typeof createMockCommandContext>;
  capturedOptions: () => Array<{ value: string; label: string }>;
} {
  const ctx = createMockCommandContext({ slashCommands: [settingsCmd] });
  const handleCtx: SettingsHandleContext = {
    ctx,
    settingsCommand: settingsCmd as any,
    resolveEffect: (() => {
      throw new Error('resolveEffect not expected in these tests');
    }) as any,
  };
  const capturedOptions = () => {
    const call = ctx._spies.setActiveCommand!.mock.calls.at(-1);
    return (call?.[0]?.options ?? []) as Array<{
      value: string;
      label: string;
    }>;
  };
  return { handleCtx, ctx, capturedOptions };
}

function readCli(testDir: string): Record<string, unknown> {
  const p = join(testDir, '.kiro', 'settings', 'cli.json');
  if (!existsSync(p)) return {};
  return JSON.parse(readFileSync(p, 'utf-8'));
}

describe('settings subcommands registry', () => {
  let testDir: string;
  let originalHome: string | undefined;

  beforeEach(() => {
    testDir = join(
      tmpdir(),
      `kiro-settings-sub-${process.pid}-${Date.now()}-${Math.random().toString(36).slice(2)}`
    );
    mkdirSync(join(testDir, '.kiro', 'settings'), { recursive: true });
    originalHome = process.env.HOME;
    process.env.HOME = testDir;
  });

  afterEach(() => {
    process.env.HOME = originalHome;
    try {
      rmSync(testDir, { recursive: true, force: true });
    } catch {
      /* ignore */
    }
  });

  describe('registration (reachability guard)', () => {
    it('registers the interrupt behaviour entry and both leaf options', () => {
      // These are the exact values the dispatcher routes on. If any goes
      // missing, the option becomes unreachable from `/settings <sub>`.
      expect(findSettingsSubcommand('terminal:interrupt')).toBeDefined();
      expect(findSettingsSubcommand('terminal:interrupt:steer')).toBeDefined();
      expect(findSettingsSubcommand('terminal:interrupt:queue')).toBeDefined();
    });

    it('lists interrupt behaviour inside the terminal submenu', () => {
      const { handleCtx, capturedOptions } = makeHandleCtx();
      findSettingsSubcommand('terminal')!.handle(handleCtx);
      const values = capturedOptions().map((o) => o.value);
      expect(values).toContain('terminal:newlines');
      expect(values).toContain('terminal:interrupt');
    });

    it('offers steer and queue inside the interrupt submenu', () => {
      const { handleCtx, capturedOptions } = makeHandleCtx();
      findSettingsSubcommand('terminal:interrupt')!.handle(handleCtx);
      const values = capturedOptions().map((o) => o.value);
      expect(values).toEqual([
        'terminal:interrupt:steer',
        'terminal:interrupt:queue',
      ]);
    });
  });

  describe('KAS gating', () => {
    it('omits interrupt behaviour from the terminal submenu on KAS', () => {
      const { handleCtx, ctx, capturedOptions } = makeHandleCtx();
      ctx.agentEngine = 'kas';
      findSettingsSubcommand('terminal')!.handle(handleCtx);
      const values = capturedOptions().map((o) => o.value);
      expect(values).toContain('terminal:newlines');
      expect(values).not.toContain('terminal:interrupt');
    });

    it('still lists interrupt behaviour on v2', () => {
      const { handleCtx, capturedOptions } = makeHandleCtx();
      findSettingsSubcommand('terminal')!.handle(handleCtx);
      const values = capturedOptions().map((o) => o.value);
      expect(values).toContain('terminal:interrupt');
    });

    it('guards the direct terminal:interrupt entry on KAS', () => {
      const { handleCtx, ctx } = makeHandleCtx();
      ctx.agentEngine = 'kas';
      findSettingsSubcommand('terminal:interrupt')!.handle(handleCtx);
      expect(ctx._spies.setActiveCommand!).not.toHaveBeenCalled();
      expect(ctx._spies.showAlert!).toHaveBeenCalled();
    });
  });

  describe('persistence', () => {
    it('selecting steer persists chat.defaultInterruptBehavior=steer', () => {
      const { handleCtx } = makeHandleCtx();
      findSettingsSubcommand('terminal:interrupt:steer')!.handle(handleCtx);
      expect(readCli(testDir)['chat.defaultInterruptBehavior']).toBe('steer');
    });

    it('selecting queue persists chat.defaultInterruptBehavior=queue', () => {
      const { handleCtx } = makeHandleCtx();
      findSettingsSubcommand('terminal:interrupt:queue')!.handle(handleCtx);
      expect(readCli(testDir)['chat.defaultInterruptBehavior']).toBe('queue');
    });

    it('surfaces a confirmation alert when applying a mode', () => {
      const { handleCtx, ctx } = makeHandleCtx();
      findSettingsSubcommand('terminal:interrupt:queue')!.handle(handleCtx);
      expect(ctx._spies.showAlert!).toHaveBeenCalled();
      const [message] = ctx._spies.showAlert!.mock.calls.at(-1)!;
      expect(String(message).toLowerCase()).toContain('queue');
    });
  });

  describe('current-value marker', () => {
    it('marks the active mode with a dot in the interrupt submenu', () => {
      const { handleCtx: applyCtx } = makeHandleCtx();
      findSettingsSubcommand('terminal:interrupt:queue')!.handle(applyCtx);

      const { handleCtx, capturedOptions } = makeHandleCtx();
      findSettingsSubcommand('terminal:interrupt')!.handle(handleCtx);
      const queueOption = capturedOptions().find(
        (o) => o.value === 'terminal:interrupt:queue'
      );
      expect(queueOption?.label).toContain('●');
    });
  });

  it('has no duplicate subcommand values', () => {
    const seen = new Set<string>();
    for (const sub of settingsSubcommands) {
      expect(seen.has(sub.value)).toBe(false);
      seen.add(sub.value);
    }
  });
});
