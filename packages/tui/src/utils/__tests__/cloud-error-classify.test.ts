import { describe, expect, it } from 'bun:test';
import {
  classifyCloudError,
  cloudErrorGuidance,
} from '../cloud-error-classify';

describe('classifyCloudError', () => {
  it('classifies the proven KAS↔BFF skew error as version_skew', () => {
    expect(classifyCloudError(new Error('UnknownOperationException'))).toBe(
      'version_skew'
    );
    expect(
      classifyCloudError({
        message: 'Unknown ext method: _kiro/sourceProviders/list',
      })
    ).toBe('version_skew');
  });

  it('classifies the relay-flattened UnknownOperationException as version_skew', () => {
    // An un-routed operation arrives flattened to `<op>: UnknownError` —
    // the exception class name is dropped in transit.
    expect(classifyCloudError(new Error('createSession: UnknownError'))).toBe(
      'version_skew'
    );
    expect(classifyCloudError(new Error('listSessions: UnknownError'))).toBe(
      'version_skew'
    );
  });

  it('classifies the observed relay truncation as stream_truncated', () => {
    expect(
      classifyCloudError(
        new Error('relayed stream ended without a done frame (truncated)')
      )
    ).toBe('stream_truncated');
    expect(classifyCloudError(new Error('stream truncated'))).toBe(
      'stream_truncated'
    );
  });

  it('does NOT classify non-stream truncation as stream_truncated', () => {
    // Bare "truncated" without stream context must fall through so it can't
    // shadow other branches (e.g. a gateway trimming a payload).
    expect(classifyCloudError(new Error('payload truncated by gateway'))).toBe(
      'other'
    );
  });

  it('classifies throttle shapes as throttling', () => {
    expect(classifyCloudError(new Error('ThrottlingException'))).toBe(
      'throttling'
    );
    expect(classifyCloudError(new Error('Too Many Requests'))).toBe(
      'throttling'
    );
    expect(classifyCloudError({ message: 'quota exceeded for account' })).toBe(
      'throttling'
    );
  });

  it('classifies auth failures as auth', () => {
    expect(classifyCloudError(new Error('UnauthorizedException'))).toBe('auth');
    expect(classifyCloudError({ message: 'Access denied' })).toBe('auth');
    expect(classifyCloudError({ message: 'token expired' })).toBe('auth');
    // Compact AWS credential exception class names (no spaces).
    expect(classifyCloudError(new Error('ExpiredTokenException'))).toBe('auth');
    expect(classifyCloudError(new Error('InvalidClientTokenId'))).toBe('auth');
  });

  it('classifies DNS ENOTFOUND as network, not not_found', () => {
    expect(
      classifyCloudError(new Error('getaddrinfo ENOTFOUND api.example.com'))
    ).toBe('network');
  });

  it('prefers the JSON-RPC data payload over the generic message', () => {
    // Agents put the real cause in `data` while `message` stays "Internal error"
    expect(
      classifyCloudError({
        message: 'Internal error',
        data: { details: 'UnknownOperationException from BFF' },
      })
    ).toBe('version_skew');
  });

  it('classifies not-found, timeout, network, server errors', () => {
    expect(classifyCloudError(new Error('Session not found'))).toBe(
      'not_found'
    );
    expect(classifyCloudError(new Error('request timed out'))).toBe('timeout');
    expect(classifyCloudError(new Error('ECONNRESET'))).toBe('network');
    expect(classifyCloudError({ message: 'Service Unavailable' })).toBe(
      'server_error'
    );
  });

  it('falls back to other for unknown or empty errors', () => {
    expect(classifyCloudError(new Error('something odd'))).toBe('other');
    expect(classifyCloudError(undefined)).toBe('other');
    expect(classifyCloudError(null)).toBe('other');
  });
});

describe('cloudErrorGuidance', () => {
  it('gives actionable guidance for the operable kinds', () => {
    expect(cloudErrorGuidance('throttling')).toContain('retry');
    expect(cloudErrorGuidance('auth')).toContain('kiro-cli login');
    expect(cloudErrorGuidance('version_skew')).toContain('out of sync');
    expect(cloudErrorGuidance('stream_truncated')).toContain('resume');
  });

  it('returns undefined for kinds with no generic guidance', () => {
    expect(cloudErrorGuidance('not_found')).toBeUndefined();
    expect(cloudErrorGuidance('other')).toBeUndefined();
  });
});
