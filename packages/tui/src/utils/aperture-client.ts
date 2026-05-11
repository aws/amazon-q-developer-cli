/**
 * Aperture Data Ingestion client.
 *
 * Aperture is AWS's internal feedback/research service. This client submits
 * survey form data to the prod Ingestion write API.
 *
 * Endpoint: https://ingestion.aperture-public-api.feedback.console.aws.dev/form
 * Override: `KIRO_APERTURE_URL=<url>` for local testing.
 *
 * Notes
 * -----
 *   - The endpoint is rate-limited to 100 requests / 5 min per source IP.
 *   - All failures return a typed `ApertureError` instead of throwing, so
 *     the survey UX path can stay noise-free.
 */

import { logger } from './logger.js';
import { getCliVersion } from './version.js';

const APERTURE_URL =
  'https://ingestion.aperture-public-api.feedback.console.aws.dev/form';

/** Error categories surfaced to callers. */
export type ApertureErrorKind =
  | 'network' // fetch threw / DNS / TLS / connection
  | 'timeout' // request exceeded timeout
  | 'aborted' // caller cancelled via AbortSignal
  | 'rate_limited' // 429 from the service
  | 'client_error' // 4xx other than 429
  | 'server_error' // 5xx
  | 'invalid_response'; // response body wasn't the expected shape

export interface ApertureError {
  kind: ApertureErrorKind;
  /** HTTP status if one was received. */
  status?: number;
  /** Short human-readable message, safe to log. Never contains PII. */
  message: string;
  /** Original cause, when available. */
  cause?: unknown;
}

export interface ApertureSubmitResult {
  ok: boolean;
  /** Present on success — Aperture's response payload. Shape TBD. */
  data?: unknown;
  /** Present on failure. */
  error?: ApertureError;
}

export interface ApertureSubmitOptions {
  /** Caller-controlled abort. Surfaces as kind: 'aborted'. */
  signal?: AbortSignal;
  /** Override the default 10s request timeout. */
  timeoutMs?: number;
  /** Extra headers merged with the defaults (e.g. auth / correlation IDs). */
  headers?: Record<string, string>;
  /** Override resolved stage URL. Takes precedence over the env var. */
  url?: string;
}

/** Resolves the target URL. Override via `KIRO_APERTURE_URL` for testing. */
export function resolveApertureUrl(override?: string): string {
  if (override && override.length > 0) return override;
  const envUrl = process.env.KIRO_APERTURE_URL;
  if (envUrl && envUrl.length > 0) return envUrl;
  return APERTURE_URL;
}

const DEFAULT_TIMEOUT_MS = 10_000;

/** Fetch with an abort-linked timeout that cleans up on success or failure. */
async function fetchWithTimeout(
  url: string,
  init: RequestInit,
  timeoutMs: number,
  externalSignal: AbortSignal | undefined
): Promise<Response> {
  const timer = new AbortController();
  const timeoutHandle = setTimeout(() => timer.abort('timeout'), timeoutMs);

  // Merge the caller's signal with the timeout's signal so either can cancel.
  const composite = externalSignal
    ? mergeSignals(externalSignal, timer.signal)
    : timer.signal;

  try {
    return await fetch(url, { ...init, signal: composite });
  } finally {
    clearTimeout(timeoutHandle);
  }
}

/** Minimal AbortSignal merge — fires when either input aborts. */
function mergeSignals(a: AbortSignal, b: AbortSignal): AbortSignal {
  // Node 20 ships AbortSignal.any(). Fall back to a manual listener when it
  // isn't available (older runtimes, browser polyfills in tests).
  const anyFn = (
    AbortSignal as unknown as {
      any?: (signals: AbortSignal[]) => AbortSignal;
    }
  ).any;
  if (typeof anyFn === 'function') return anyFn([a, b]);

  const ctl = new AbortController();
  const onAbort = (src: AbortSignal) => () => ctl.abort(src.reason);
  if (a.aborted) ctl.abort(a.reason);
  else a.addEventListener('abort', onAbort(a), { once: true });
  if (b.aborted) ctl.abort(b.reason);
  else b.addEventListener('abort', onAbort(b), { once: true });
  return ctl.signal;
}

/**
 * Submit a `Form` payload to the Aperture ingestion endpoint.
 *
 * The payload is typed as `unknown` because we don't yet have the schema;
 * callers should build the object per Aperture's Form contract (e.g. the
 * `formId` + `responses` layout from the linked API doc). Once the schema is
 * finalized, we can narrow this to a concrete interface.
 */
export async function submitForm(
  payload: unknown,
  options: ApertureSubmitOptions = {}
): Promise<ApertureSubmitResult> {
  const url = resolveApertureUrl(options.url);
  const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS;

  const headers: Record<string, string> = {
    'Content-Type': 'application/json',
    // Help the Aperture team identify client traffic without phoning home PII.
    'User-Agent': `kiro-cli/${getCliVersion()}`,
    ...options.headers,
  };

  let body: string;
  try {
    body = JSON.stringify(payload);
  } catch (err) {
    return {
      ok: false,
      error: {
        kind: 'invalid_response',
        message: 'Payload could not be serialized to JSON',
        cause: err,
      },
    };
  }

  let response: Response;
  try {
    response = await fetchWithTimeout(
      url,
      { method: 'POST', headers, body },
      timeoutMs,
      options.signal
    );
  } catch (err) {
    if (options.signal?.aborted) {
      return {
        ok: false,
        error: {
          kind: 'aborted',
          message: 'Request was aborted by the caller',
          cause: err,
        },
      };
    }
    if (err instanceof Error && /abort/i.test(err.message ?? '')) {
      return {
        ok: false,
        error: {
          kind: 'timeout',
          message: `Request timed out after ${timeoutMs}ms`,
          cause: err,
        },
      };
    }
    logger.warn('[aperture] network error submitting form', err);
    return {
      ok: false,
      error: {
        kind: 'network',
        message: 'Network error contacting Aperture',
        cause: err,
      },
    };
  }

  if (response.status === 429) {
    return {
      ok: false,
      error: {
        kind: 'rate_limited',
        status: 429,
        message: 'Aperture rate limit reached (100 requests / 5 min per IP)',
      },
    };
  }

  if (response.status >= 500) {
    return {
      ok: false,
      error: {
        kind: 'server_error',
        status: response.status,
        message: `Aperture server error (${response.status})`,
      },
    };
  }

  if (response.status >= 400) {
    // Try to surface the error body to help future debugging — but don't
    // fail if it isn't JSON.
    let detail = '';
    try {
      const text = await response.text();
      detail = text.slice(0, 512);
    } catch {
      // ignore
    }
    return {
      ok: false,
      error: {
        kind: 'client_error',
        status: response.status,
        message: `Aperture rejected form (${response.status})${detail ? `: ${detail}` : ''}`,
      },
    };
  }

  // 2xx — try to parse JSON, but tolerate an empty body.
  try {
    const text = await response.text();
    const data = text.length > 0 ? JSON.parse(text) : {};
    return { ok: true, data };
  } catch (err) {
    return {
      ok: false,
      error: {
        kind: 'invalid_response',
        status: response.status,
        message: 'Aperture returned a non-JSON body',
        cause: err,
      },
    };
  }
}

// Test hooks — exported for unit tests only.
export const _internals = {
  APERTURE_URL,
  DEFAULT_TIMEOUT_MS,
};
