import { describe, expect, test } from 'vitest';
import { isRemoteEnvironment } from '../browser.js';

describe('isRemoteEnvironment', () => {
  test('false on a plain local environment', () => {
    expect(isRemoteEnvironment({})).toBe(false);
  });

  test.each(['SSH_CLIENT', 'SSH_CONNECTION', 'SSH_TTY'])(
    'true when %s is set',
    (name) => {
      expect(isRemoteEnvironment({ [name]: '10.0.0.1 5000 22' })).toBe(true);
    }
  );

  test('a set-but-empty SSH variable still counts (is_some semantics)', () => {
    expect(isRemoteEnvironment({ SSH_TTY: '' })).toBe(true);
  });

  test.each(['KIRO_FAKE_IS_REMOTE', 'Q_FAKE_IS_REMOTE'])(
    'true when the %s test override is set',
    (name) => {
      expect(isRemoteEnvironment({ [name]: '1' })).toBe(true);
    }
  );

  test('unrelated env vars do not trip the check', () => {
    expect(
      isRemoteEnvironment({ TERM: 'xterm-256color', HOME: '/home/u' })
    ).toBe(false);
  });

  // WSL is not remote: wslview reaches the Windows-side browser there.
  test('WSL-ish env without SSH vars is not remote', () => {
    expect(
      isRemoteEnvironment({ WSL_DISTRO_NAME: 'Ubuntu', WSLENV: 'PATH/l' })
    ).toBe(false);
  });
});
