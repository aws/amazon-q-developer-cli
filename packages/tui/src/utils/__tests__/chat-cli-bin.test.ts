import { describe, it, expect, beforeEach, afterEach } from 'bun:test';

import { resolveChatCliBinFromEnv, NOT_FOUND_MESSAGE } from '../chat-cli-bin';

const ENV_BIN = '/versioned/install/dir/kiro-cli-chat';
const HOME = '/home/user';
const LOCAL_CHAT_BIN = '/home/user/.local/bin/kiro-cli-chat';
const LOCAL_LAUNCHER = '/home/user/.local/bin/kiro-cli';
const USR_LOCAL_LAUNCHER = '/usr/local/bin/kiro-cli';
const HOMEBREW_LAUNCHER = '/opt/homebrew/bin/kiro-cli';

let originalBin: string | undefined;

beforeEach(() => {
  originalBin = process.env.KIRO_CHAT_CLI_BIN;
});

afterEach(() => {
  if (originalBin === undefined) delete process.env.KIRO_CHAT_CLI_BIN;
  else process.env.KIRO_CHAT_CLI_BIN = originalBin;
});

describe('resolveChatCliBinFromEnv', () => {
  it('returns the env path when it exists on disk', () => {
    process.env.KIRO_CHAT_CLI_BIN = ENV_BIN;
    const bin = resolveChatCliBinFromEnv({
      exists: (p) => p === ENV_BIN,
      homeDir: () => HOME,
    });
    expect(bin).toBe(ENV_BIN);
  });

  it('throws the canonical message when the env var is unset', () => {
    delete process.env.KIRO_CHAT_CLI_BIN;
    expect(() =>
      resolveChatCliBinFromEnv({ exists: () => true, homeDir: () => HOME })
    ).toThrow(NOT_FOUND_MESSAGE);
  });

  it('throws the canonical message when the env var is empty', () => {
    process.env.KIRO_CHAT_CLI_BIN = '';
    expect(() =>
      resolveChatCliBinFromEnv({ exists: () => true, homeDir: () => HOME })
    ).toThrow(NOT_FOUND_MESSAGE);
  });

  it('prefers the same program the env named, reinstalled under ~/.local/bin', () => {
    process.env.KIRO_CHAT_CLI_BIN = ENV_BIN;
    const bin = resolveChatCliBinFromEnv({
      exists: (p) => p === LOCAL_CHAT_BIN || p === LOCAL_LAUNCHER,
      homeDir: () => HOME,
    });
    expect(bin).toBe(LOCAL_CHAT_BIN);
  });

  it('falls back to the ~/.local/bin launcher when the named program is gone', () => {
    process.env.KIRO_CHAT_CLI_BIN = ENV_BIN;
    const bin = resolveChatCliBinFromEnv({
      exists: (p) => p === LOCAL_LAUNCHER,
      homeDir: () => HOME,
    });
    expect(bin).toBe(LOCAL_LAUNCHER);
  });

  it('falls back to /usr/local/bin then Homebrew launcher locations', () => {
    process.env.KIRO_CHAT_CLI_BIN = ENV_BIN;
    const usrLocal = resolveChatCliBinFromEnv({
      exists: (p) => p === USR_LOCAL_LAUNCHER || p === HOMEBREW_LAUNCHER,
      homeDir: () => HOME,
    });
    expect(usrLocal).toBe(USR_LOCAL_LAUNCHER);

    const homebrew = resolveChatCliBinFromEnv({
      exists: (p) => p === HOMEBREW_LAUNCHER,
      homeDir: () => HOME,
    });
    expect(homebrew).toBe(HOMEBREW_LAUNCHER);
  });

  it('returns the env path unchanged when no candidate exists', () => {
    process.env.KIRO_CHAT_CLI_BIN = ENV_BIN;
    const bin = resolveChatCliBinFromEnv({
      exists: () => false,
      homeDir: () => HOME,
    });
    expect(bin).toBe(ENV_BIN);
  });

  it('never returns a relative or bare command name', () => {
    process.env.KIRO_CHAT_CLI_BIN = ENV_BIN;
    const probed: string[] = [];
    const bin = resolveChatCliBinFromEnv({
      exists: (p) => {
        probed.push(p);
        return false;
      },
      homeDir: () => HOME,
    });
    expect(bin.startsWith('/')).toBe(true);
    for (const p of probed) {
      expect(p.startsWith('/')).toBe(true);
    }
  });

  it('probes the filesystem only once for the common case of a valid env path', () => {
    process.env.KIRO_CHAT_CLI_BIN = ENV_BIN;
    const probed: string[] = [];
    resolveChatCliBinFromEnv({
      exists: (p) => {
        probed.push(p);
        return p === ENV_BIN;
      },
      homeDir: () => HOME,
    });
    expect(probed).toEqual([ENV_BIN]);
  });
});
