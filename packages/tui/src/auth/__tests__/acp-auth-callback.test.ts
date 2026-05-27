/**
 * Unit tests for the `acp-auth-callback` capability.
 *
 * Covers argv assembly, JSON-on-stdout extraction, and error path
 * classification using a fake spawner. The real binary is never invoked
 * here. End-to-end coverage of the `chat _ get-kas-token` subcommand
 * lives in the Rust crate.
 */

import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import {
  createGetAccessTokenCapability,
  type AsyncSpawner,
} from '../acp-auth-callback';

interface SpawnerHarness {
  spawner: AsyncSpawner;
  calls: Array<{ cmd: string; args: string[] }>;
}

function makeSpawner(result: {
  status: number;
  stdout: string;
  stderr?: string;
  error?: Error;
}): SpawnerHarness {
  const calls: SpawnerHarness['calls'] = [];
  const spawner: AsyncSpawner = (cmd, args) => {
    calls.push({ cmd, args });
    return Promise.resolve({
      status: result.status,
      stdout: result.stdout,
      stderr: result.stderr ?? '',
      ...(result.error ? { error: result.error } : {}),
    });
  };
  return { spawner, calls };
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

describe('createGetAccessTokenCapability', () => {
  it('registers under the `_kiro/auth/getAccessToken` method', () => {
    const { spawner } = makeSpawner({ status: 0, stdout: '' });
    const cap = createGetAccessTokenCapability(spawner);
    expect(cap.method).toBe('_kiro/auth/getAccessToken');
    // 'other' so KiroClient wires the handler via extMethodHandlers and
    // surfaces the key under `_meta.kiro` for capability advertisement.
    expect(cap.type).toBe('other');
  });

  it('invokes `chat _ get-kas-token` with the canonical argv', async () => {
    const { spawner, calls } = makeSpawner({
      status: 0,
      stdout:
        '{"success":true,"accessToken":"at","expiresAt":"2099-01-01T00:00:00Z","profileArn":"arn:aws:iam::1:profile/x"}',
    });
    const cap = createGetAccessTokenCapability(spawner);

    const response = await cap.handler({});

    expect(response).toEqual({
      accessToken: 'at',
      expiresAt: '2099-01-01T00:00:00Z',
      profileArn: 'arn:aws:iam::1:profile/x',
    });
    expect(calls).toHaveLength(1);
    expect(calls[0]!.cmd).toBe(FAKE_BIN);
    // KAS sends `{}` so the argv is fixed: no `--reason` or
    // `--current-expires-at`. Per `acp-callback-auth-provider.ts`:
    // "The request payload is intentionally empty".
    expect(calls[0]!.args).toEqual(['chat', '_', 'get-kas-token']);
  });

  it('ignores any extra fields KAS may send (forward compat)', async () => {
    const { spawner, calls } = makeSpawner({
      status: 0,
      stdout:
        '{"success":true,"accessToken":"at","expiresAt":"2099-01-01T00:00:00Z","profileArn":"arn:aws:iam::1:profile/x"}',
    });
    const cap = createGetAccessTokenCapability(spawner);

    // Type-cast: future KAS revisions may add hint fields; the host is free
    // to ignore them. This test pins the "empty argv" guarantee.
    await cap.handler({ futureHint: 'whatever' } as unknown as Parameters<
      typeof cap.handler
    >[0]);

    expect(calls[0]!.args).toEqual(['chat', '_', 'get-kas-token']);
  });

  it('forwards profileArn when present in the JSON response', async () => {
    const { spawner } = makeSpawner({
      status: 0,
      stdout: JSON.stringify({
        success: true,
        accessToken: 'at',
        expiresAt: '2099-01-01T00:00:00Z',
        profileArn: 'arn:aws:iam::123:profile/x',
      }),
    });
    const cap = createGetAccessTokenCapability(spawner);

    const response = await cap.handler({});

    expect(response).toEqual({
      accessToken: 'at',
      expiresAt: '2099-01-01T00:00:00Z',
      profileArn: 'arn:aws:iam::123:profile/x',
    });
  });

  it('strips host-internal `success` field; ignores any extra fields chat-cli might emit', async () => {
    const { spawner } = makeSpawner({
      status: 0,
      stdout: JSON.stringify({
        success: true,
        accessToken: 'at',
        expiresAt: '2099-01-01T00:00:00Z',
        profileArn: 'arn:aws:iam::123:profile/x',
        // Forward-compat: any extra fields the host emits that aren't part
        // of the KAS wire contract MUST be dropped on the way out.
        someOtherField: 'whatever',
      }),
    });
    const cap = createGetAccessTokenCapability(spawner);

    const response = await cap.handler({});

    // Wire contract is `{accessToken, expiresAt, profileArn}`.
    expect(Object.keys(response).sort()).toEqual([
      'accessToken',
      'expiresAt',
      'profileArn',
    ]);
  });

  it('throws when the JSON response is missing profileArn', async () => {
    // chat-cli always emits profileArn (its `AcpCallbackToken.profile_arn`
    // is `String`, not `Option<String>`). Missing -> contract violation.
    const { spawner } = makeSpawner({
      status: 0,
      stdout: JSON.stringify({
        success: true,
        accessToken: 'at',
        expiresAt: '2099-01-01T00:00:00Z',
      }),
    });
    const cap = createGetAccessTokenCapability(spawner);

    await expect(cap.handler({})).rejects.toThrow(/missing profileArn/);
  });

  it('throws the parsed error message when the binary exits non-zero', async () => {
    const { spawner } = makeSpawner({
      status: 1,
      stdout:
        '{"success":false,"error":"You are not logged in. Please log in with `kiro-cli login`."}',
    });
    const cap = createGetAccessTokenCapability(spawner);

    await expect(cap.handler({})).rejects.toThrow(
      'You are not logged in. Please log in with `kiro-cli login`.'
    );
  });

  it('throws when the spawn itself fails', async () => {
    const { spawner } = makeSpawner({
      status: -1,
      stdout: '',
      error: Object.assign(new Error('spawn ENOENT'), { code: 'ENOENT' }),
    });
    const cap = createGetAccessTokenCapability(spawner);

    await expect(cap.handler({})).rejects.toThrow(/ENOENT/);
  });

  it('throws when stdout has no parseable JSON', async () => {
    const { spawner } = makeSpawner({
      status: 1,
      stdout: 'totally broken output with no json',
    });
    const cap = createGetAccessTokenCapability(spawner);

    await expect(cap.handler({})).rejects.toThrow();
  });

  it('parses the JSON success even with preceding stdout log lines', async () => {
    const { spawner } = makeSpawner({
      status: 0,
      stdout:
        '[2025-05-21T17:00:00Z INFO  some::module] preflight ok\n' +
        '[2025-05-21T17:00:00Z DEBUG some::module] computed hash\n' +
        '{"success":true,"accessToken":"at","expiresAt":"2099-01-01T00:00:00Z","profileArn":"arn:aws:iam::1:profile/x"}',
    });
    const cap = createGetAccessTokenCapability(spawner);

    const response = await cap.handler({});

    expect(response).toEqual({
      accessToken: 'at',
      expiresAt: '2099-01-01T00:00:00Z',
      profileArn: 'arn:aws:iam::1:profile/x',
    });
  });

  it('throws when KIRO_CHAT_CLI_BIN is not set', async () => {
    delete process.env.KIRO_CHAT_CLI_BIN;
    const { spawner, calls } = makeSpawner({
      status: 0,
      stdout:
        '{"success":true,"accessToken":"at","expiresAt":"2099-01-01T00:00:00Z"}',
    });
    const cap = createGetAccessTokenCapability(spawner);

    await expect(cap.handler({})).rejects.toThrow(/KIRO_CHAT_CLI_BIN/);
    // Spawner must not be invoked when the binary path can't be resolved.
    expect(calls).toHaveLength(0);
  });

  it('throws when JSON response is missing accessToken', async () => {
    const { spawner } = makeSpawner({
      status: 0,
      stdout: '{"success":true,"expiresAt":"2099-01-01T00:00:00Z"}',
    });
    const cap = createGetAccessTokenCapability(spawner);

    await expect(cap.handler({})).rejects.toThrow();
  });

  it('throws when JSON response is missing expiresAt', async () => {
    const { spawner } = makeSpawner({
      status: 0,
      stdout: '{"success":true,"accessToken":"at"}',
    });
    const cap = createGetAccessTokenCapability(spawner);

    await expect(cap.handler({})).rejects.toThrow();
  });
});
