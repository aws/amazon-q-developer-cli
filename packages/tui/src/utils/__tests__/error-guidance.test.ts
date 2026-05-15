import { describe, it, expect } from 'bun:test';
import {
  simplifyErrorMessage,
  getAuthErrorGuidance,
  getSessionErrorGuidance,
  getNetworkErrorGuidance,
  getPermissionErrorGuidance,
  getMcpErrorGuidance,
  getRateLimitGuidance,
  getToolErrorGuidance,
  detectErrorCategory,
  getErrorGuidance,
  CLI_BINARY_NAME,
} from '../error-guidance';

describe('simplifyErrorMessage', () => {
  it('returns "Not authenticated" for no token errors', () => {
    expect(simplifyErrorMessage('No token found in cache')).toBe(
      'Not authenticated'
    );
  });

  it('returns "Session expired" for token expired errors', () => {
    expect(simplifyErrorMessage('Token expired at 2026-01-01')).toBe(
      'Session expired'
    );
  });

  it('returns network error for "error sending request"', () => {
    expect(simplifyErrorMessage('error sending request to backend')).toBe(
      'Network error - unable to connect'
    );
  });

  it('returns "Network error - unable to reach service" for i/o error with URL', () => {
    expect(
      simplifyErrorMessage('I/O error for url (https://api.example.com)')
    ).toBe('Network error - unable to reach service');
  });

  it('returns "Network error" for i/o error without URL', () => {
    expect(simplifyErrorMessage('i/o error reading response')).toBe(
      'Network error'
    );
  });

  it('returns "Network error" for "io error" variant', () => {
    expect(simplifyErrorMessage('io error: broken pipe')).toBe('Network error');
  });

  it('strips "Encountered an error in the response stream: " prefix', () => {
    expect(
      simplifyErrorMessage(
        'Encountered an error in the response stream: something broke'
      )
    ).toBe('something broke');
  });

  it('strips "An unknown error occurred: " prefix', () => {
    expect(simplifyErrorMessage('An unknown error occurred: weird thing')).toBe(
      'weird thing'
    );
  });

  it('extracts root cause from dispatch failure messages', () => {
    expect(
      simplifyErrorMessage(
        'Dispatch failure - service unavailable - token invalid'
      )
    ).toBe('Token invalid');
  });

  it('returns original message when no simplification applies', () => {
    expect(simplifyErrorMessage('Something unexpected happened')).toBe(
      'Something unexpected happened'
    );
  });

  it('handles dispatch failure with single part (no dash separator)', () => {
    expect(simplifyErrorMessage('Dispatch failure')).toBe('Dispatch failure');
  });
});

describe('getAuthErrorGuidance', () => {
  it('returns login guidance for no_token', () => {
    const g = getAuthErrorGuidance('no_token');
    expect(g.message).toContain('login');
    expect(g.recoveryAction).toBe(`${CLI_BINARY_NAME} login`);
  });

  it('returns re-authenticate guidance for token_expired', () => {
    const g = getAuthErrorGuidance('token_expired');
    expect(g.message).toContain('expired');
    expect(g.recoveryAction).toBeDefined();
  });

  it('returns timeout guidance for oauth_timeout', () => {
    const g = getAuthErrorGuidance('oauth_timeout');
    expect(g.message).toContain('timed out');
  });

  it('returns security validation guidance for oauth_state_mismatch', () => {
    const g = getAuthErrorGuidance('oauth_state_mismatch');
    expect(g.message).toContain('security validation');
  });

  it('returns social login guidance for social_auth_failure', () => {
    const g = getAuthErrorGuidance('social_auth_failure');
    expect(g.message).toContain('Social login');
  });

  it('returns contact support for unauthorized_client', () => {
    const g = getAuthErrorGuidance('unauthorized_client');
    expect(g.message).toContain('contact support');
    expect(g.recoveryAction).toBeUndefined();
  });

  it('returns default auth guidance for unauthorized', () => {
    const g = getAuthErrorGuidance('unauthorized');
    expect(g.message).toContain('Authentication required');
  });

  it('returns default auth guidance for unknown type', () => {
    const g = getAuthErrorGuidance('some_unknown_type' as any);
    expect(g.message).toContain('Authentication required');
  });
});

describe('getSessionErrorGuidance', () => {
  it('returns locked message with PID when provided', () => {
    const g = getSessionErrorGuidance('session_locked', 12345);
    expect(g.message).toContain('PID: 12345');
  });

  it('returns locked message without PID when not provided', () => {
    const g = getSessionErrorGuidance('session_locked');
    expect(g.message).toContain('locked by another process');
    expect(g.message).not.toContain('PID');
  });

  it('returns not found guidance', () => {
    const g = getSessionErrorGuidance('session_not_found');
    expect(g.message).toContain('new session will be created');
  });

  it('returns io_error guidance', () => {
    const g = getSessionErrorGuidance('io_error');
    expect(g.message).toContain('file permissions');
  });

  it('returns json_parse_error guidance', () => {
    const g = getSessionErrorGuidance('json_parse_error');
    expect(g.message).toContain('corrupted');
  });

  it('returns default guidance for unknown session error', () => {
    const g = getSessionErrorGuidance('unknown_type' as any);
    expect(g.message).toContain('session error occurred');
  });
});

describe('getNetworkErrorGuidance', () => {
  it('returns timeout guidance', () => {
    const g = getNetworkErrorGuidance('Connection timeout after 30s');
    expect(g.message).toContain('timed out');
  });

  it('returns connection refused guidance for ECONNREFUSED', () => {
    const g = getNetworkErrorGuidance('ECONNREFUSED 127.0.0.1:3000');
    expect(g.message).toContain('Connection refused');
  });

  it('returns connection refused guidance for "connection refused"', () => {
    const g = getNetworkErrorGuidance('connection refused by server');
    expect(g.message).toContain('Connection refused');
  });

  it('returns DNS guidance for ENOTFOUND', () => {
    const g = getNetworkErrorGuidance('ENOTFOUND api.example.com');
    expect(g.message).toContain('resolve host');
  });

  it('returns DNS guidance for dns errors', () => {
    const g = getNetworkErrorGuidance('dns resolution failed');
    expect(g.message).toContain('resolve host');
  });

  it('returns generic network guidance for other errors', () => {
    const g = getNetworkErrorGuidance('some network issue');
    expect(g.message).toContain('Network error occurred');
  });
});

describe('getPermissionErrorGuidance', () => {
  it('returns permission denied for EACCES', () => {
    const g = getPermissionErrorGuidance('EACCES: /etc/passwd');
    expect(g.message).toContain('Permission denied');
  });

  it('returns permission denied for "permission denied"', () => {
    const g = getPermissionErrorGuidance('permission denied writing file');
    expect(g.message).toContain('Permission denied');
  });

  it('returns elevated privileges for EPERM', () => {
    const g = getPermissionErrorGuidance('EPERM: operation not permitted');
    expect(g.message).toContain('elevated privileges');
  });

  it('returns generic permission guidance for other errors', () => {
    const g = getPermissionErrorGuidance('some access issue');
    expect(g.message).toContain('access rights');
  });
});

describe('getMcpErrorGuidance', () => {
  it('returns not found guidance for ENOENT', () => {
    const g = getMcpErrorGuidance('my-server', 'ENOENT: no such file');
    expect(g.message).toContain('executable not found');
    expect(g.message).toContain('my-server');
  });

  it('returns not found guidance for "not found"', () => {
    const g = getMcpErrorGuidance('builder-mcp', 'binary not found');
    expect(g.message).toContain('executable not found');
  });

  it('returns timeout guidance', () => {
    const g = getMcpErrorGuidance('slow-server', 'initialization timeout');
    expect(g.message).toContain('timed out');
    expect(g.message).toContain('slow-server');
  });

  it('returns generic MCP guidance for other errors', () => {
    const g = getMcpErrorGuidance('my-mcp', 'crashed unexpectedly');
    expect(g.message).toContain('failed to initialize');
  });
});

describe('getRateLimitGuidance', () => {
  it('returns rate limit message', () => {
    const g = getRateLimitGuidance();
    expect(g.message).toContain('Rate limit exceeded');
  });
});

describe('getToolErrorGuidance', () => {
  it('returns timeout guidance for tool timeout', () => {
    const g = getToolErrorGuidance('shell', 'execution timeout after 60s');
    expect(g.message).toContain('timed out');
    expect(g.message).toContain('shell');
  });

  it('returns permission guidance for denied tools', () => {
    const g = getToolErrorGuidance('fs_write', 'permission denied');
    expect(g.message).toContain('denied permission');
  });

  it('returns generic tool guidance for other errors', () => {
    const g = getToolErrorGuidance('grep', 'regex parse error');
    expect(g.message).toContain('encountered an error');
    expect(g.message).toContain('conversation can continue');
  });
});

describe('detectErrorCategory', () => {
  it('detects auth from "no token"', () => {
    expect(detectErrorCategory('No token found')).toBe('auth');
  });

  it('detects auth from "token expired"', () => {
    expect(detectErrorCategory('Token expired')).toBe('auth');
  });

  it('detects auth from "unauthorized"', () => {
    expect(detectErrorCategory('Unauthorized access')).toBe('auth');
  });

  it('detects auth from "not authenticated"', () => {
    expect(detectErrorCategory('User not authenticated')).toBe('auth');
  });

  it('detects auth from "authentication required"', () => {
    expect(detectErrorCategory('Authentication required for this action')).toBe(
      'auth'
    );
  });

  it('detects auth from dispatch failure with token', () => {
    expect(detectErrorCategory('Dispatch failure - token refresh failed')).toBe(
      'auth'
    );
  });

  it('detects auth from generic "auth" keyword', () => {
    expect(detectErrorCategory('auth service unavailable')).toBe('auth');
  });

  it('detects auth from "login" keyword', () => {
    expect(detectErrorCategory('please login first')).toBe('auth');
  });

  it('detects session errors', () => {
    expect(detectErrorCategory('session not found')).toBe('session');
  });

  it('detects network from "network"', () => {
    expect(detectErrorCategory('network unreachable')).toBe('network');
  });

  it('detects network from "connection"', () => {
    expect(detectErrorCategory('connection reset')).toBe('network');
  });

  it('detects network from "timeout"', () => {
    expect(detectErrorCategory('request timeout')).toBe('network');
  });

  it('detects network from "econnrefused"', () => {
    expect(detectErrorCategory('ECONNREFUSED')).toBe('network');
  });

  it('detects network from "enotfound"', () => {
    expect(detectErrorCategory('ENOTFOUND')).toBe('network');
  });

  it('detects network from "i/o error"', () => {
    expect(detectErrorCategory('i/o error on socket')).toBe('network');
  });

  it('detects network from "error sending request"', () => {
    expect(detectErrorCategory('error sending request')).toBe('network');
  });

  it('detects permission from "permission"', () => {
    expect(detectErrorCategory('permission denied')).toBe('permission');
  });

  it('detects permission from "eacces"', () => {
    expect(detectErrorCategory('EACCES')).toBe('permission');
  });

  it('detects permission from "eperm"', () => {
    expect(detectErrorCategory('EPERM')).toBe('permission');
  });

  it('detects mcp errors', () => {
    expect(detectErrorCategory('mcp server crashed')).toBe('mcp');
  });

  it('detects rate_limit from "rate limit"', () => {
    expect(detectErrorCategory('rate limit exceeded')).toBe('rate_limit');
  });

  it('detects rate_limit from "throttle"', () => {
    expect(detectErrorCategory('request throttled')).toBe('rate_limit');
  });

  it('detects rate_limit from "throttling"', () => {
    expect(detectErrorCategory('throttling applied')).toBe('rate_limit');
  });

  it('detects tool errors', () => {
    expect(detectErrorCategory('tool execution failed')).toBe('tool');
  });

  it('returns unknown for unrecognized messages', () => {
    expect(detectErrorCategory('something completely different')).toBe(
      'unknown'
    );
  });
});

describe('getErrorGuidance', () => {
  it('returns auth guidance for auth errors', () => {
    const g = getErrorGuidance('no token found');
    expect(g.message).toContain('login');
    expect(g.recoveryAction).toBeDefined();
  });

  it('returns network guidance for network errors', () => {
    const g = getErrorGuidance('connection timeout');
    expect(g.message).toContain('timed out');
  });

  it('returns permission guidance for permission errors', () => {
    const g = getErrorGuidance('EACCES: permission denied');
    expect(g.message).toContain('Permission denied');
  });

  it('returns rate limit guidance', () => {
    const g = getErrorGuidance('rate limit exceeded');
    expect(g.message).toContain('Rate limit');
  });

  it('returns session guidance', () => {
    const g = getErrorGuidance('session corrupted');
    expect(g.message).toContain('session error');
  });

  it('returns mcp guidance', () => {
    const g = getErrorGuidance('mcp server failed');
    expect(g.message).toContain('MCP server error');
  });

  it('returns tool guidance', () => {
    const g = getErrorGuidance('tool crashed');
    expect(g.message).toContain('tool execution error');
  });

  it('returns generic guidance for unknown errors', () => {
    const g = getErrorGuidance('something weird');
    expect(g.message).toContain('An error occurred');
  });
});
