/**
 * Unit tests for the `acp-auth-callback` capability.
 *
 * The host is responsible for translating any failure path - spawn
 * error, non-zero exit, parse failure, error envelope, unexpected
 * response kind - into a single user-facing string. Internal
 * diagnostics live in `logger` only and never reach the user-visible
 * ACP error. These tests pin that invariant.
 */

import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import {
  AUTH_ERROR_USER_FACING,
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

const SUCCESS_STDOUT = JSON.stringify({
  kind: 'getKasToken',
  data: {
    accessToken: 'at',
    expiresAt: '2099-01-01T00:00:00Z',
    profileArn: 'arn:aws:iam::1:profile/x',
  },
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
      stdout: SUCCESS_STDOUT,
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
    // `--current-expires-at`.
    expect(calls[0]!.args).toEqual(['chat', '_', 'get-kas-token']);
  });

  it('ignores any extra fields KAS may send (forward compat)', async () => {
    const { spawner, calls } = makeSpawner({
      status: 0,
      stdout: SUCCESS_STDOUT,
    });
    const cap = createGetAccessTokenCapability(spawner);

    // Type-cast: future KAS revisions may add hint fields; the host is free
    // to ignore them. This test pins the "empty argv" guarantee.
    await cap.handler({ futureHint: 'whatever' } as unknown as Parameters<
      typeof cap.handler
    >[0]);

    expect(calls[0]!.args).toEqual(['chat', '_', 'get-kas-token']);
  });

  it('returns the wire-contract shape exactly (no extra fields)', async () => {
    const { spawner } = makeSpawner({
      status: 0,
      stdout: SUCCESS_STDOUT,
    });
    const cap = createGetAccessTokenCapability(spawner);

    const response = await cap.handler({});

    expect(Object.keys(response).sort()).toEqual([
      'accessToken',
      'expiresAt',
      'profileArn',
    ]);
  });

  it('forwards authMethod (external_idp) so KAS can set the TokenType header', async () => {
    const { spawner } = makeSpawner({
      status: 0,
      stdout: JSON.stringify({
        kind: 'getKasToken',
        data: {
          accessToken: 'at',
          expiresAt: '2099-01-01T00:00:00Z',
          profileArn: 'arn:aws:codewhisperer:us-east-1:123:profile/x',
          authMethod: 'external_idp',
        },
      }),
    });
    const cap = createGetAccessTokenCapability(spawner);

    const response = await cap.handler({});

    expect(response).toEqual({
      accessToken: 'at',
      expiresAt: '2099-01-01T00:00:00Z',
      profileArn: 'arn:aws:codewhisperer:us-east-1:123:profile/x',
      authMethod: 'external_idp',
    } as unknown as typeof response);
  });

  it('omits authMethod when chat-cli does not emit one (Builder ID / IdC / Social)', async () => {
    const { spawner } = makeSpawner({
      status: 0,
      stdout: JSON.stringify({
        kind: 'getKasToken',
        data: {
          accessToken: 'at',
          expiresAt: '2099-01-01T00:00:00Z',
          profileArn: 'arn:aws:iam::123:profile/x',
        },
      }),
    });
    const cap = createGetAccessTokenCapability(spawner);

    const response = await cap.handler({});

    expect(Object.keys(response).sort()).toEqual([
      'accessToken',
      'expiresAt',
      'profileArn',
    ]);
  });

  it('throws the user-facing string on an `error` envelope', async () => {
    const { spawner } = makeSpawner({
      status: 1,
      stdout:
        '{"kind":"error","data":{"message":"You are not logged in. Please log in with `kiro-cli login`."}}',
    });
    const cap = createGetAccessTokenCapability(spawner);

    await expect(cap.handler({})).rejects.toThrow(AUTH_ERROR_USER_FACING);
  });

  it('throws the user-facing string when the spawn itself fails', async () => {
    const { spawner } = makeSpawner({
      status: -1,
      stdout: '',
      error: Object.assign(new Error('spawn ENOENT'), { code: 'ENOENT' }),
    });
    const cap = createGetAccessTokenCapability(spawner);

    await expect(cap.handler({})).rejects.toThrow(AUTH_ERROR_USER_FACING);
  });

  it('throws the user-facing string when stdout has no parseable JSON', async () => {
    const { spawner } = makeSpawner({
      status: 1,
      stdout: 'totally broken output with no json',
    });
    const cap = createGetAccessTokenCapability(spawner);

    await expect(cap.handler({})).rejects.toThrow(AUTH_ERROR_USER_FACING);
  });

  it('throws the user-facing string on an unexpected response kind', async () => {
    const { spawner } = makeSpawner({
      status: 0,
      stdout: '{"kind":"ensureSession","data":{"sessionId":"sess_xxx"}}',
    });
    const cap = createGetAccessTokenCapability(spawner);

    await expect(cap.handler({})).rejects.toThrow(AUTH_ERROR_USER_FACING);
  });

  it('parses the JSON success even with preceding stdout log lines', async () => {
    const { spawner } = makeSpawner({
      status: 0,
      stdout:
        '[2025-05-21T17:00:00Z INFO  some::module] preflight ok\n' +
        '[2025-05-21T17:00:00Z DEBUG some::module] computed hash\n' +
        SUCCESS_STDOUT,
    });
    const cap = createGetAccessTokenCapability(spawner);

    const response = await cap.handler({});

    expect(response).toEqual({
      accessToken: 'at',
      expiresAt: '2099-01-01T00:00:00Z',
      profileArn: 'arn:aws:iam::1:profile/x',
    });
  });

  it('throws the user-facing string when KIRO_CHAT_CLI_BIN is not set', async () => {
    delete process.env.KIRO_CHAT_CLI_BIN;
    const { spawner, calls } = makeSpawner({
      status: 0,
      stdout: SUCCESS_STDOUT,
    });
    const cap = createGetAccessTokenCapability(spawner);

    await expect(cap.handler({})).rejects.toThrow(AUTH_ERROR_USER_FACING);
    // Spawner must not be invoked when the binary path can't be resolved.
    expect(calls).toHaveLength(0);
  });

  it('does NOT leak underlying chat-cli error message to the caller', async () => {
    const internalDetails =
      'auth refresh failed: SQLite error: database is locked';
    const { spawner } = makeSpawner({
      status: 1,
      stdout: `{"kind":"error","data":{"message":"${internalDetails}"}}`,
    });
    const cap = createGetAccessTokenCapability(spawner);

    let thrownMessage = '';
    try {
      await cap.handler({});
    } catch (e) {
      thrownMessage = e instanceof Error ? e.message : String(e);
    }
    expect(thrownMessage).toBe(AUTH_ERROR_USER_FACING);
    expect(thrownMessage).not.toContain('SQLite');
    expect(thrownMessage).not.toContain('database is locked');
  });
});
