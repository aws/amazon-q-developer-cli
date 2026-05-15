import { describe, it, expect } from 'bun:test';
import { extractRpcErrorMessage } from '../error-handling';

/**
 * Mirrors `@agentclientprotocol/sdk`'s `RequestError`:
 *   new RequestError(code, message, data)
 * where `data` may be a string, object, or any other value per JSON-RPC 2.0.
 */
class RequestErrorStub extends Error {
  code: number;
  data: unknown;
  constructor(code: number, message: string, data: unknown) {
    super(message);
    this.name = 'RequestError';
    this.code = code;
    this.data = data;
  }
}

describe('extractRpcErrorMessage', () => {
  // --- KAS-shaped RequestError (the bug we're fixing) ---

  it('extracts data.details from ACP RequestError.internalError', () => {
    // This is exactly the shape KAS produces:
    //   RequestError.internalError({ details: "..." })
    const err = new RequestErrorStub(-32603, 'Internal error', {
      details:
        'No auth token found. Log in via the Kiro VS Code extension first. Expected: /Users/x/.aws/sso/cache/kiro-auth-token-cli.json',
    });
    expect(extractRpcErrorMessage(err)).toBe(
      'No auth token found. Log in via the Kiro VS Code extension first. Expected: /Users/x/.aws/sso/cache/kiro-auth-token-cli.json'
    );
  });

  it('extracts details from plain object with same shape as serialized RPC error', () => {
    // When the error comes across the wire as a plain object (not an Error
    // instance), we should still recover the details.
    const err = {
      code: -32603,
      message: 'Internal error',
      data: { details: 'backend blew up' },
    };
    expect(extractRpcErrorMessage(err)).toBe('backend blew up');
  });

  // --- data as string ---

  it('extracts data when data is a non-empty string', () => {
    const err = new RequestErrorStub(-32603, 'Internal error', 'useful detail');
    expect(extractRpcErrorMessage(err)).toBe('useful detail');
  });

  it('ignores empty string data and falls back', () => {
    const err = new RequestErrorStub(-32603, 'Something real', '');
    expect(extractRpcErrorMessage(err)).toBe('Something real');
  });

  // --- data as object variants ---

  it('prefers data.details over data.message when both present', () => {
    const err = new RequestErrorStub(-32603, 'Internal error', {
      details: 'the details',
      message: 'the message',
    });
    expect(extractRpcErrorMessage(err)).toBe('the details');
  });

  it('uses data.message when data.details is missing', () => {
    const err = new RequestErrorStub(-32603, 'Internal error', {
      message: 'nested message',
    });
    expect(extractRpcErrorMessage(err)).toBe('nested message');
  });

  it('serializes object data when no details/message but message is generic', () => {
    const err = new RequestErrorStub(-32603, 'Internal error', {
      reason: 'x',
      code: 42,
    });
    const result = extractRpcErrorMessage(err);
    expect(result).toContain('Internal error');
    expect(result).toContain('reason');
    expect(result).toContain('42');
  });

  it('skips empty object data when falling back', () => {
    const err = new RequestErrorStub(-32603, 'Useful message', {});
    expect(extractRpcErrorMessage(err)).toBe('Useful message');
  });

  // --- message-only fallbacks ---

  it('returns error.message when no data and message is specific', () => {
    const err = new Error('Prompt already in progress');
    expect(extractRpcErrorMessage(err)).toBe('Prompt already in progress');
  });

  it('returns generic JSON-RPC message only when nothing else is available', () => {
    const err = new RequestErrorStub(-32603, 'Internal error', undefined);
    expect(extractRpcErrorMessage(err)).toBe('Internal error');
  });

  it('recognizes all standard generic JSON-RPC messages', () => {
    const genericMessages = [
      'Internal error',
      'Parse error',
      'Invalid request',
      'Invalid params',
      'Method not found',
    ];
    for (const msg of genericMessages) {
      const err = new RequestErrorStub(-32603, msg, {
        details: 'real cause',
      });
      expect(extractRpcErrorMessage(err)).toBe('real cause');
    }
  });

  // --- primitives and edge cases ---

  it('returns string errors as-is', () => {
    expect(extractRpcErrorMessage('plain string error')).toBe(
      'plain string error'
    );
  });

  it('returns fallback for empty string', () => {
    expect(extractRpcErrorMessage('', 'my fallback')).toBe('my fallback');
  });

  it('returns fallback for null', () => {
    expect(extractRpcErrorMessage(null, 'my fallback')).toBe('my fallback');
  });

  it('returns fallback for undefined', () => {
    expect(extractRpcErrorMessage(undefined, 'my fallback')).toBe(
      'my fallback'
    );
  });

  it('returns default fallback when none specified', () => {
    expect(extractRpcErrorMessage(null)).toBe('Unknown error');
  });

  it('returns fallback for numbers', () => {
    expect(extractRpcErrorMessage(42, 'fallback')).toBe('fallback');
  });

  // --- robustness ---

  it('handles data with non-string details', () => {
    const err = new RequestErrorStub(-32603, 'Internal error', {
      details: 12345, // not a string — should not crash, should fall through
    });
    const result = extractRpcErrorMessage(err);
    // Falls through to JSON serialization since message is generic
    expect(result).toContain('12345');
  });

  it('handles circular data objects gracefully', () => {
    const data: Record<string, unknown> = { foo: 'bar' };
    data.self = data;
    const err = new RequestErrorStub(-32603, 'Internal error', data);
    // Should not throw; returns message fallback since JSON.stringify fails
    const result = extractRpcErrorMessage(err);
    expect(typeof result).toBe('string');
    expect(result.length).toBeGreaterThan(0);
  });

  it('handles plain object with string data (not Error instance)', () => {
    const err = {
      code: -32603,
      message: 'Internal error',
      data: 'direct string data',
    };
    expect(extractRpcErrorMessage(err)).toBe('direct string data');
  });
});

import {
  getErrorMessage,
  isNetworkError,
  isPermissionError,
  formatErrorForUser,
} from '../error-handling';

describe('getErrorMessage', () => {
  it('extracts message from Error instance', () => {
    expect(getErrorMessage(new Error('boom'))).toBe('boom');
  });

  it('returns string errors as-is', () => {
    expect(getErrorMessage('something failed')).toBe('something failed');
  });

  it('returns fallback for non-string non-Error', () => {
    expect(getErrorMessage(42)).toBe('An unknown error occurred');
    expect(getErrorMessage(null)).toBe('An unknown error occurred');
    expect(getErrorMessage(undefined)).toBe('An unknown error occurred');
  });
});

describe('isNetworkError', () => {
  it('detects network keyword', () => {
    expect(isNetworkError(new Error('network unreachable'))).toBe(true);
  });

  it('detects connection keyword', () => {
    expect(isNetworkError('connection reset')).toBe(true);
  });

  it('detects timeout', () => {
    expect(isNetworkError('request timeout')).toBe(true);
  });

  it('detects ECONNREFUSED', () => {
    expect(isNetworkError('ECONNREFUSED 127.0.0.1')).toBe(true);
  });

  it('detects ENOTFOUND', () => {
    expect(isNetworkError('ENOTFOUND api.example.com')).toBe(true);
  });

  it('returns false for non-network errors', () => {
    expect(isNetworkError('file not found')).toBe(false);
  });
});

describe('isPermissionError', () => {
  it('detects permission keyword', () => {
    expect(isPermissionError('permission denied')).toBe(true);
  });

  it('detects EACCES', () => {
    expect(isPermissionError('EACCES: /etc/shadow')).toBe(true);
  });

  it('detects EPERM', () => {
    expect(isPermissionError('EPERM: operation not permitted')).toBe(true);
  });

  it('returns false for non-permission errors', () => {
    expect(isPermissionError('file not found')).toBe(false);
  });
});

describe('formatErrorForUser', () => {
  it('formats network errors with connection prefix', () => {
    const result = formatErrorForUser('connection timeout');
    expect(result).toContain('Connection error');
    expect(result).toContain('check your network');
  });

  it('formats permission errors with permission prefix', () => {
    const result = formatErrorForUser('EACCES: /tmp/file');
    expect(result).toContain('Permission error');
    expect(result).toContain('file permissions');
  });

  it('returns plain message for other errors', () => {
    expect(formatErrorForUser('something broke')).toBe('something broke');
  });
});
