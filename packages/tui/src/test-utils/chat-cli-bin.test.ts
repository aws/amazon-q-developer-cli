import { afterEach, beforeEach, describe, expect, it } from 'bun:test';
import * as fs from 'node:fs';
import * as os from 'node:os';
import * as path from 'node:path';

import { resolveChatCliBin } from './chat-cli-bin';

const BINARY_NAME = process.platform === 'win32' ? 'chat_cli.exe' : 'chat_cli';
const originalEnv = {
  CARGO_TARGET_DIR: process.env.CARGO_TARGET_DIR,
  CI: process.env.CI,
  KIRO_CHAT_CLI_BIN: process.env.KIRO_CHAT_CLI_BIN,
};

let tempDir: string;
let debugBin: string;
let explicitBin: string;

function restoreEnv(name: keyof typeof originalEnv): void {
  const value = originalEnv[name];
  if (value === undefined) {
    delete process.env[name];
  } else {
    process.env[name] = value;
  }
}

beforeEach(() => {
  tempDir = fs.mkdtempSync(path.join(os.tmpdir(), 'chat-cli-bin-'));
  debugBin = path.join(tempDir, 'target', 'debug', BINARY_NAME);
  explicitBin = path.join(tempDir, 'artifact', BINARY_NAME);
  fs.mkdirSync(path.dirname(debugBin), { recursive: true });
  fs.mkdirSync(path.dirname(explicitBin), { recursive: true });
  fs.writeFileSync(debugBin, '');
  fs.writeFileSync(explicitBin, '');
  process.env.CARGO_TARGET_DIR = path.join(tempDir, 'target');
  process.env.KIRO_CHAT_CLI_BIN = explicitBin;
});

afterEach(() => {
  restoreEnv('CARGO_TARGET_DIR');
  restoreEnv('CI');
  restoreEnv('KIRO_CHAT_CLI_BIN');
  fs.rmSync(tempDir, { recursive: true, force: true });
});

describe('resolveChatCliBin', () => {
  it('uses the explicit certification artifact in CI', () => {
    process.env.CI = '1';

    expect(resolveChatCliBin()).toBe(explicitBin);
  });

  it('prefers a workspace debug build outside CI', () => {
    process.env.CI = 'false';

    expect(resolveChatCliBin()).toBe(debugBin);
  });
});
