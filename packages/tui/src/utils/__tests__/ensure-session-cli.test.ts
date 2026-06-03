/**
 * Unit tests for `ensure-session-cli`. This wrapper contributes
 * argv assembly on top of `chat-internal-cli`; shared-layer
 * behavior (binary resolution, JSON parsing, error envelopes,
 * etc.) is exercised in `chat-internal-cli.test.ts`. Tests here
 * assert argv shape behaviorally via flag/value pairs, so argv
 * reordering does not break tests.
 */

import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import { ensureSession, type AsyncSpawner } from '../ensure-session-cli';

interface SpawnerHarness {
  spawner: AsyncSpawner;
  calls: Array<{ cmd: string; args: string[] }>;
}

function makeSpawner(stdout: string, status = 0): SpawnerHarness {
  const calls: SpawnerHarness['calls'] = [];
  const spawner: AsyncSpawner = async (cmd, args) => {
    calls.push({ cmd, args });
    return { status, stdout, stderr: '' };
  };
  return { spawner, calls };
}

/// Find the value following `flag` in a positional argv. Returns
/// undefined when the flag is absent. Behavioral counterpart to
/// hard-coded `args[N]` indexing.
function flagValue(args: string[], flag: string): string | undefined {
  const idx = args.indexOf(flag);
  return idx >= 0 && idx + 1 < args.length ? args[idx + 1] : undefined;
}

const FAKE_BIN = '/fake/chat_cli';
let originalBin: string | undefined;

beforeEach(() => {
  originalBin = process.env.KIRO_CHAT_CLI_BIN;
  process.env.KIRO_CHAT_CLI_BIN = FAKE_BIN;
});

afterEach(() => {
  if (originalBin === undefined) delete process.env.KIRO_CHAT_CLI_BIN;
  else process.env.KIRO_CHAT_CLI_BIN = originalBin;
});

describe('ensureSession argv', () => {
  it('passes every input as a flag-value pair to the binary', async () => {
    const { spawner, calls } = makeSpawner(
      '{"success":true,"sessionId":"sess_uuid"}'
    );
    const result = await ensureSession(
      {
        sourceFormat: 'auto',
        sourceSessionId: 'uuid',
        targetFormat: 'kas',
        cwd: '/work',
      },
      spawner
    );

    expect(result).toEqual({ ok: true, sessionId: 'sess_uuid' });
    expect(calls).toHaveLength(1);
    expect(calls[0]!.cmd).toBe(FAKE_BIN);

    const args = calls[0]!.args;
    // Subcommand routing.
    expect(args.slice(0, 3)).toEqual(['chat', '_', 'ensure-session']);
    // Inputs surface as flag-value pairs; specific positions are
    // implementation detail.
    expect(flagValue(args, '--source-format')).toBe('auto');
    expect(flagValue(args, '--source-session-id')).toBe('uuid');
    expect(flagValue(args, '--target-format')).toBe('kas');
    expect(flagValue(args, '--cwd')).toBe('/work');
  });

  it('passes through each source-format variant unchanged', async () => {
    for (const sourceFormat of ['auto', 'classic', 'v2', 'kas'] as const) {
      const { spawner, calls } = makeSpawner(
        '{"success":true,"sessionId":"sess_uuid"}'
      );
      await ensureSession(
        {
          sourceFormat,
          sourceSessionId: 'uuid',
          targetFormat: 'kas',
          cwd: '/work',
        },
        spawner
      );
      expect(flagValue(calls[0]!.args, '--source-format')).toBe(sourceFormat);
    }
  });
});
