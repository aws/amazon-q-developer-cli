export const getErrorMessage = (error: unknown): string => {
  if (error instanceof Error) {
    return error.message;
  }
  if (typeof error === 'string') {
    return error;
  }
  return 'An unknown error occurred';
};

/**
 * JSON-RPC 2.0 generic error messages that carry no user-useful info on their
 * own. When these appear, callers should prefer detail extracted from `data`.
 */
const GENERIC_RPC_MESSAGES = new Set([
  'Internal error',
  'Parse error',
  'Invalid request',
  'Invalid params',
  'Method not found',
]);

/**
 * Extract a user-useful message from a JSON-RPC / ACP RequestError.
 *
 * ACP's `RequestError` (from `@agentclientprotocol/sdk`) follows JSON-RPC 2.0:
 * it has `code`, `message`, and an optional `data` field. Per the JSON-RPC
 * spec, `data` may be a string, object, array, or primitive.
 *
 * Agents commonly surface the real cause inside `data`. For example, the KAS
 * agent calls `RequestError.internalError({ details: "..." })`, producing:
 *   { code: -32603, message: "Internal error", data: { details: "..." } }
 *
 * This helper prefers, in order:
 *   1. `data.details` (string) — convention used by `RequestError.internalError`
 *   2. `data.message` (string)
 *   3. `data` when it is itself a string
 *   4. `error.message`, unless it is a generic JSON-RPC placeholder
 *   5. A serialized form of `data`, if present
 *   6. A generic fallback
 *
 * Generic JSON-RPC messages (e.g. "Internal error") are suppressed when any
 * more specific information is available; they fall through only if nothing
 * better can be extracted.
 */
export const extractRpcErrorMessage = (
  error: unknown,
  fallback: string = 'Unknown error'
): string => {
  if (error === null || error === undefined) {
    return fallback;
  }

  if (typeof error === 'string') {
    return error || fallback;
  }

  if (typeof error !== 'object') {
    return fallback;
  }

  const err = error as { message?: unknown; data?: unknown };

  // 1-3: Try to extract the most useful info from `data` first, since agents
  // typically use `data` to convey the real cause while `message` stays the
  // generic JSON-RPC code label (e.g. "Internal error").
  const { data } = err;
  if (data !== undefined && data !== null) {
    if (typeof data === 'string') {
      if (data) return data;
    } else if (typeof data === 'object') {
      const dataObj = data as { details?: unknown; message?: unknown };
      if (typeof dataObj.details === 'string' && dataObj.details) {
        return dataObj.details;
      }
      if (typeof dataObj.message === 'string' && dataObj.message) {
        return dataObj.message;
      }
    }
  }

  // 4: Fall back to `error.message`, but skip it if it's just the generic
  // JSON-RPC category label — those are useless to users.
  const message = typeof err.message === 'string' ? err.message : '';
  if (message && !GENERIC_RPC_MESSAGES.has(message)) {
    return message;
  }

  // 5: Last resort — if there's a `data` object we couldn't extract from,
  // serialize it so at least some info surfaces to the user.
  if (data !== undefined && data !== null && typeof data === 'object') {
    try {
      const serialized = JSON.stringify(data);
      if (serialized && serialized !== '{}' && serialized !== '[]') {
        return `${message || fallback}: ${serialized}`;
      }
    } catch {
      // Circular or otherwise non-serializable — ignore.
    }
  }

  // 6: Give back the generic message if that's all we have; otherwise fallback.
  return message || fallback;
};

export const isNetworkError = (error: unknown): boolean => {
  const message = getErrorMessage(error).toLowerCase();
  return (
    message.includes('network') ||
    message.includes('connection') ||
    message.includes('timeout') ||
    message.includes('econnrefused') ||
    message.includes('enotfound')
  );
};

export const isPermissionError = (error: unknown): boolean => {
  const message = getErrorMessage(error).toLowerCase();
  return (
    message.includes('permission') ||
    message.includes('eacces') ||
    message.includes('eperm')
  );
};

export const formatErrorForUser = (error: unknown): string => {
  const message = getErrorMessage(error);

  if (isNetworkError(error)) {
    return `Connection error: ${message}. Please check your network connection.`;
  }

  if (isPermissionError(error)) {
    return `Permission error: ${message}. Please check file permissions.`;
  }

  return message;
};
