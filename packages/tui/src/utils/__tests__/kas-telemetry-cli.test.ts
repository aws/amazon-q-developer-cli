import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import type { AsyncSpawner } from '../chat-internal-cli';
import type { KasTelemetryEvent } from '../kas-telemetry-cli';

// @ts-expect-error - Bun query-string import isolates this module from sibling test mocks.
const { emitKasTelemetry } = await import('../kas-telemetry-cli?unit-test');

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

function flagValue(args: string[], flag: string): string | undefined {
  const idx = args.indexOf(flag);
  return idx >= 0 && idx + 1 < args.length ? args[idx + 1] : undefined;
}

const FAKE_BIN = '/fake/chat_cli';
let originalBin: string | undefined;
let originalTestMode: string | undefined;

beforeEach(() => {
  originalBin = process.env.KIRO_CHAT_CLI_BIN;
  originalTestMode = process.env.KIRO_TEST_MODE;
  process.env.KIRO_CHAT_CLI_BIN = FAKE_BIN;
  delete process.env.KIRO_TEST_MODE;
});

afterEach(() => {
  if (originalBin === undefined) delete process.env.KIRO_CHAT_CLI_BIN;
  else process.env.KIRO_CHAT_CLI_BIN = originalBin;
  if (originalTestMode === undefined) delete process.env.KIRO_TEST_MODE;
  else process.env.KIRO_TEST_MODE = originalTestMode;
});

describe('emitKasTelemetry', () => {
  it('routes KAS telemetry payloads through the host telemetry subcommand', async () => {
    const { spawner, calls } = makeSpawner(
      '{"kind":"emitTelemetry","data":{}}'
    );
    const event: KasTelemetryEvent = 'kas-process-health';
    const payload = {
      sessionId: 'kas-session-1',
      agentKind: 'kas',
      rssMb: 128,
      version: '2.4.0',
      platform: 'darwin',
    };

    const result = await emitKasTelemetry(event, payload, spawner);

    expect(result).toEqual({ ok: true });
    expect(calls).toHaveLength(1);
    expect(calls[0]!.cmd).toBe(FAKE_BIN);
    expect(calls[0]!.args.slice(0, 3)).toEqual(['chat', '_', 'emit-telemetry']);
    expect(flagValue(calls[0]!.args, '--event')).toBe(event);
    expect(JSON.parse(flagValue(calls[0]!.args, '--payload-json')!)).toEqual(
      payload
    );
  });

  it('skips real host spawns in TUI test mode', async () => {
    process.env.KIRO_TEST_MODE = 'true';

    const result = await emitKasTelemetry('kas-chat-session-started', {
      sessionId: 'kas-session-1',
    });

    expect(result).toEqual({ ok: true });
  });
});
