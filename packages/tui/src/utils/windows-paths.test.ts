import { test, expect, afterEach } from 'bun:test';
import { system32Path } from './windows-paths.js';

/**
 * Verifies the shared System32 path helper used to invoke Windows system tools
 * by their absolute path (CWE-426 hardening). Leak-free: no module mocking —
 * we only set/restore the real `process.env.SystemRoot`.
 */
const originalSystemRoot = process.env.SystemRoot;

afterEach(() => {
  if (originalSystemRoot === undefined) {
    delete process.env.SystemRoot;
  } else {
    process.env.SystemRoot = originalSystemRoot;
  }
});

test('falls back to C:\\Windows when SystemRoot is unset', () => {
  delete process.env.SystemRoot;
  expect(system32Path('reg.exe')).toBe('C:\\Windows\\System32\\reg.exe');
});

test('honors the SystemRoot env var', () => {
  process.env.SystemRoot = 'D:\\WinNT';
  expect(system32Path('reg.exe')).toBe('D:\\WinNT\\System32\\reg.exe');
});

test('joins multiple segments with backslashes', () => {
  process.env.SystemRoot = 'C:\\Windows';
  expect(system32Path('WindowsPowerShell', 'v1.0', 'powershell.exe')).toBe(
    'C:\\Windows\\System32\\WindowsPowerShell\\v1.0\\powershell.exe'
  );
});
