import { win32 } from 'path';

/**
 * Absolute path to a binary under %SystemRoot%\System32 (defaults to
 * C:\Windows). Windows system tools (reg.exe, powershell.exe, notepad.exe,
 * where.exe) are invoked by their full path so a binary planted in the CWD or
 * opened project cannot hijack them at pre-trust startup (CWE-426 untrusted
 * search path).
 *
 * Uses `path.win32.join` so it yields correct backslash paths regardless of the
 * host OS the tests run on.
 */
export function system32Path(...segments: string[]): string {
  const root = process.env.SystemRoot ?? 'C:\\Windows';
  return win32.join(root, 'System32', ...segments);
}
