import { describe, test, expect, beforeEach, afterEach, mock } from 'bun:test';

import {
  submitForm,
  resolveApertureUrl,
  _internals,
} from './aperture-client.js';

/**
 * These tests mock global `fetch` so no network traffic is ever produced.
 * We restore the original fetch after each case so parallel test files
 * aren't affected.
 */

type FetchArgs = [input: string | URL, init?: RequestInit];

function mockFetch(
  responder: (url: string, init: RequestInit) => Response | Promise<Response>
) {
  const calls: FetchArgs[] = [];
  const fn = mock(async (input: string | URL, init?: RequestInit) => {
    calls.push([input, init]);
    return responder(String(input), init ?? {});
  });
  globalThis.fetch = fn as unknown as typeof fetch;
  return { fn, calls };
}

describe('aperture-client: URL resolution', () => {
  const savedUrl = process.env.KIRO_APERTURE_URL;

  afterEach(() => {
    if (savedUrl === undefined) delete process.env.KIRO_APERTURE_URL;
    else process.env.KIRO_APERTURE_URL = savedUrl;
  });

  test('defaults to prod URL when no env vars are set', () => {
    delete process.env.KIRO_APERTURE_URL;
    expect(resolveApertureUrl()).toBe(_internals.APERTURE_URL);
  });

  test('KIRO_APERTURE_URL overrides the default', () => {
    process.env.KIRO_APERTURE_URL = 'https://example.invalid/form';
    expect(resolveApertureUrl()).toBe('https://example.invalid/form');
  });

  test('explicit override arg beats env var', () => {
    process.env.KIRO_APERTURE_URL = 'https://env.invalid/form';
    expect(resolveApertureUrl('https://explicit.invalid/form')).toBe(
      'https://explicit.invalid/form'
    );
  });
});

describe('aperture-client: submitForm happy path', () => {
  let originalFetch: typeof fetch;

  beforeEach(() => {
    originalFetch = globalThis.fetch;
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
  });

  test('posts JSON with the expected headers and body', async () => {
    const { calls } = mockFetch(() => {
      return new Response(JSON.stringify({ id: 'form-123' }), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      });
    });

    const payload = { formId: 'research-2026-q2', responses: [] };
    const result = await submitForm(payload, {
      url: 'https://example.invalid/form',
    });

    expect(result.ok).toBe(true);
    expect(result.data).toEqual({ id: 'form-123' });
    expect(calls.length).toBe(1);
    const [url, init] = calls[0]!;
    expect(url).toBe('https://example.invalid/form');
    expect(init?.method).toBe('POST');
    const headers = init?.headers as Record<string, string> | undefined;
    expect(headers?.['Content-Type']).toBe('application/json');
    expect(headers?.['User-Agent']).toMatch(/^kiro-cli\//);
    expect(init?.body).toBe(JSON.stringify(payload));
  });

  test('accepts an empty response body as success', async () => {
    mockFetch(() => new Response('', { status: 204 }));
    const result = await submitForm(
      {},
      { url: 'https://example.invalid/form' }
    );
    expect(result.ok).toBe(true);
    expect(result.data).toEqual({});
  });

  test('survey User-Agent carries the real version via getCliVersion', async () => {
    // Guards the documented bug: the survey User-Agent must report the
    // launcher-forwarded version (KIRO_VERSION_OVERRIDE), not the bundle's
    // baked-in 99.99.99-dev dev fallback.
    const saved = process.env.KIRO_VERSION_OVERRIDE;
    process.env.KIRO_VERSION_OVERRIDE = '2.4.0';
    try {
      const { calls } = mockFetch(() => new Response('{}', { status: 200 }));
      await submitForm({}, { url: 'https://example.invalid/form' });
      const headers = calls[0]?.[1]?.headers as
        | Record<string, string>
        | undefined;
      expect(headers?.['User-Agent']).toBe('kiro-cli/2.4.0');
      expect(headers?.['User-Agent']).not.toContain('99.99.99-dev');
    } finally {
      if (saved === undefined) delete process.env.KIRO_VERSION_OVERRIDE;
      else process.env.KIRO_VERSION_OVERRIDE = saved;
    }
  });
});

describe('aperture-client: error handling', () => {
  let originalFetch: typeof fetch;

  beforeEach(() => {
    originalFetch = globalThis.fetch;
  });

  afterEach(() => {
    globalThis.fetch = originalFetch;
  });

  test('maps 429 to rate_limited', async () => {
    mockFetch(() => new Response('{}', { status: 429 }));
    const result = await submitForm(
      {},
      { url: 'https://example.invalid/form' }
    );
    expect(result.ok).toBe(false);
    expect(result.error?.kind).toBe('rate_limited');
    expect(result.error?.status).toBe(429);
  });

  test('maps 500 to server_error', async () => {
    mockFetch(() => new Response('oops', { status: 500 }));
    const result = await submitForm(
      {},
      { url: 'https://example.invalid/form' }
    );
    expect(result.ok).toBe(false);
    expect(result.error?.kind).toBe('server_error');
    expect(result.error?.status).toBe(500);
  });

  test('maps 400 to client_error with detail', async () => {
    mockFetch(() => new Response('bad form', { status: 400 }));
    const result = await submitForm(
      {},
      { url: 'https://example.invalid/form' }
    );
    expect(result.ok).toBe(false);
    expect(result.error?.kind).toBe('client_error');
    expect(result.error?.status).toBe(400);
    expect(result.error?.message).toContain('bad form');
  });

  test('reports network error when fetch throws', async () => {
    globalThis.fetch = mock(async () => {
      throw new TypeError('Failed to fetch');
    }) as unknown as typeof fetch;
    const result = await submitForm(
      {},
      { url: 'https://example.invalid/form' }
    );
    expect(result.ok).toBe(false);
    expect(result.error?.kind).toBe('network');
  });

  test('reports aborted when caller cancels', async () => {
    const ac = new AbortController();
    globalThis.fetch = mock(async (_url, init) => {
      // Simulate the "abort before first byte" case
      return new Promise<Response>((_resolve, reject) => {
        const sig = (init as RequestInit | undefined)?.signal;
        sig?.addEventListener('abort', () => {
          const err = new Error('aborted');
          err.name = 'AbortError';
          reject(err);
        });
      });
    }) as unknown as typeof fetch;

    const promise = submitForm(
      {},
      { url: 'https://example.invalid/form', signal: ac.signal }
    );
    ac.abort();
    const result = await promise;
    expect(result.ok).toBe(false);
    expect(result.error?.kind).toBe('aborted');
  });

  test('reports timeout when request exceeds timeoutMs', async () => {
    globalThis.fetch = mock(async (_url, init) => {
      return new Promise<Response>((_resolve, reject) => {
        const sig = (init as RequestInit | undefined)?.signal;
        sig?.addEventListener('abort', () => {
          const err = new Error('The operation was aborted');
          err.name = 'AbortError';
          reject(err);
        });
      });
    }) as unknown as typeof fetch;

    const result = await submitForm(
      {},
      { url: 'https://example.invalid/form', timeoutMs: 5 }
    );
    expect(result.ok).toBe(false);
    expect(result.error?.kind).toBe('timeout');
  });

  test('returns invalid_response on non-JSON success body', async () => {
    mockFetch(() => new Response('<html>oops</html>', { status: 200 }));
    const result = await submitForm(
      {},
      { url: 'https://example.invalid/form' }
    );
    expect(result.ok).toBe(false);
    expect(result.error?.kind).toBe('invalid_response');
  });

  test('returns invalid_response when payload is not serializable', async () => {
    // Circular reference defeats JSON.stringify.
    const circular: Record<string, unknown> = {};
    circular.self = circular;
    const result = await submitForm(circular, {
      url: 'https://example.invalid/form',
    });
    expect(result.ok).toBe(false);
    expect(result.error?.kind).toBe('invalid_response');
  });
});
