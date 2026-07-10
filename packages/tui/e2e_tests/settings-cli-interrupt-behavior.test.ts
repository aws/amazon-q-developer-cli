/**
 * E2E test: `kiro-cli settings` CLI surface for the interrupt-behavior keys.
 *
 * These exercise the REAL `chat_cli` binary (no mocks, no TUI) end-to-end:
 * spawn `chat_cli settings <KEY> [VALUE]` against an isolated `KIRO_HOME`
 * and assert exit codes, stdout, and the persisted `cli.json`.
 *
 * Regression coverage: `chat.defaultInterruptBehavior` and
 * `chat.keybindings.toggleInterruptBehavior` were missing from the Rust
 * `Setting` enum, so the CLI rejected them as "not a valid setting" even
 * though the TUI reads them. The dual-mode feature shipped on the TUI side
 * only; the CLI write path had no coverage. This file pins that path.
 *
 * Isolation: V1 `chat_cli` resolves the settings file from `KIRO_HOME`
 * (falling back to the real home dir), so every test points `KIRO_HOME` at
 * a fresh tempdir. `KIRO_TEST_SETTINGS_PATH` is intentionally NOT used — the
 * V1 binary ignores it and would clobber the developer's real settings.
 *
 * Note on the invocation form: there is no `set`/`get` subcommand. Setting
 * is `settings <KEY> <VALUE>`, getting is `settings <KEY>`, deleting is
 * `settings --delete <KEY>`.
 */

import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';
import { requireChatCliBin } from '../src/test-utils/chat-cli-bin';

const DEFAULT_KEY = 'chat.defaultInterruptBehavior';
const TOGGLE_KEY = 'chat.keybindings.toggleInterruptBehavior';

const BIN = requireChatCliBin();

interface RunResult {
  exitCode: number | null;
  stdout: string;
  stderr: string;
}

let kiroHome: string;

/** Run `chat_cli settings ...` against the isolated KIRO_HOME. */
function runSettings(args: string[]): RunResult {
  const r = Bun.spawnSync([BIN, 'settings', ...args], {
    // Spread the real env for PATH etc., then force KIRO_HOME so the
    // settings file lands in our tempdir regardless of the dev's own
    // KIRO_HOME/HOME.
    env: {
      ...process.env,
      KIRO_HOME: kiroHome,
      KIRO_DISABLE_TELEMETRY: '1',
    },
    stdout: 'pipe',
    stderr: 'pipe',
  });
  return {
    exitCode: r.exitCode,
    stdout: r.stdout.toString(),
    stderr: r.stderr.toString(),
  };
}

/** Parse the persisted settings file, or `{}` if it does not exist yet. */
function readSettings(): Record<string, unknown> {
  const p = path.join(kiroHome, 'settings', 'cli.json');
  if (!fs.existsSync(p)) return {};
  return JSON.parse(fs.readFileSync(p, 'utf-8'));
}

beforeEach(() => {
  kiroHome = fs.mkdtempSync(path.join(os.tmpdir(), 'kiro-settings-e2e-'));
});

afterEach(() => {
  fs.rmSync(kiroHome, { recursive: true, force: true });
});

describe('kiro-cli settings — chat.defaultInterruptBehavior', () => {
  it('sets steer and persists it', () => {
    const r = runSettings([DEFAULT_KEY, 'steer']);
    expect(r.exitCode).toBe(0);
    expect(readSettings()[DEFAULT_KEY]).toBe('steer');
  });

  it('sets queue and persists it', () => {
    const r = runSettings([DEFAULT_KEY, 'queue']);
    expect(r.exitCode).toBe(0);
    expect(readSettings()[DEFAULT_KEY]).toBe('queue');
  });

  it('gets the value in plain format after a set', () => {
    expect(runSettings([DEFAULT_KEY, 'steer']).exitCode).toBe(0);
    const r = runSettings([DEFAULT_KEY]);
    expect(r.exitCode).toBe(0);
    expect(r.stdout).toContain('steer');
  });

  it('gets the value in json format after a set', () => {
    expect(runSettings([DEFAULT_KEY, 'steer']).exitCode).toBe(0);
    const r = runSettings([DEFAULT_KEY, '--format', 'json']);
    expect(r.exitCode).toBe(0);
    expect(r.stdout.trim()).toBe('"steer"');
  });

  it('overwrites a previous value (last write wins)', () => {
    expect(runSettings([DEFAULT_KEY, 'steer']).exitCode).toBe(0);
    expect(runSettings([DEFAULT_KEY, 'queue']).exitCode).toBe(0);
    expect(readSettings()[DEFAULT_KEY]).toBe('queue');
  });

  it('deletes the key', () => {
    expect(runSettings([DEFAULT_KEY, 'queue']).exitCode).toBe(0);
    const r = runSettings(['--delete', DEFAULT_KEY]);
    expect(r.exitCode).toBe(0);
    expect(readSettings()).not.toHaveProperty(DEFAULT_KEY);
  });

  it('errors when getting an unset key in plain format', () => {
    const r = runSettings([DEFAULT_KEY]);
    expect(r.exitCode).not.toBe(0);
    expect(r.stderr).toContain('No value associated with');
  });

  it('returns null when getting an unset key in json format', () => {
    const r = runSettings([DEFAULT_KEY, '--format', 'json']);
    expect(r.exitCode).toBe(0);
    expect(r.stdout.trim()).toBe('null');
  });
});

describe('kiro-cli settings — chat.keybindings.toggleInterruptBehavior', () => {
  it('sets the toggle keybinding and persists it', () => {
    const r = runSettings([TOGGLE_KEY, 'ctrl+s']);
    expect(r.exitCode).toBe(0);
    expect(readSettings()[TOGGLE_KEY]).toBe('ctrl+s');
  });

  it('gets the toggle keybinding after a set', () => {
    expect(runSettings([TOGGLE_KEY, 'ctrl+s']).exitCode).toBe(0);
    const r = runSettings([TOGGLE_KEY]);
    expect(r.exitCode).toBe(0);
    expect(r.stdout).toContain('ctrl+s');
  });

  it('deletes the toggle keybinding', () => {
    expect(runSettings([TOGGLE_KEY, 'ctrl+s']).exitCode).toBe(0);
    const r = runSettings(['--delete', TOGGLE_KEY]);
    expect(r.exitCode).toBe(0);
    expect(readSettings()).not.toHaveProperty(TOGGLE_KEY);
  });
});

describe('kiro-cli settings — interrupt-behavior key validation', () => {
  // The core regression: these keys must be RECOGNIZED, not rejected as
  // "invalid setting". This is exactly what was broken before the enum fix.
  it('accepts both keys instead of rejecting them as invalid settings', () => {
    const def = runSettings([DEFAULT_KEY, 'steer']);
    expect(def.exitCode).toBe(0);
    expect(def.stderr).not.toContain('is not a valid setting');

    const toggle = runSettings([TOGGLE_KEY, 'ctrl+s']);
    expect(toggle.exitCode).toBe(0);
    expect(toggle.stderr).not.toContain('is not a valid setting');
  });

  // Contrast: an unregistered key IS rejected, proving key validation is
  // active and the two keys above pass because they were registered.
  it('still rejects a genuinely unknown key', () => {
    const r = runSettings(['chat.totallyBogusKey', 'x']);
    expect(r.exitCode).not.toBe(0);
    expect(r.stderr).toContain('is not a valid setting');
  });

  // Documents current behavior: the CLI validates the KEY but not the
  // VALUE, so an out-of-domain value is accepted and stored verbatim. The
  // TUI is responsible for falling back to steer for unrecognized values.
  // If value validation is added later, this test should be updated.
  it('accepts an out-of-domain value (no value validation today)', () => {
    const r = runSettings([DEFAULT_KEY, 'bogus']);
    expect(r.exitCode).toBe(0);
    expect(readSettings()[DEFAULT_KEY]).toBe('bogus');
  });
});
