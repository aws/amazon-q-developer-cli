/**
 * Real-clipboard round-trip tests for `readClipboardImage`.
 *
 * These tests actually run the platform-specific reader on whatever OS
 * Bun is executing on:
 *   - macOS  → osascript
 *   - Linux  → wl-copy / xclip (skipped when there's no display server)
 *   - Windows → PowerShell + System.Windows.Forms.Clipboard
 *
 * Each platform block first stages a known PNG into the system clipboard
 * using the inverse of the platform tool, then reads it back through our
 * code and asserts on the round-tripped bytes.
 *
 * We use `Bun.spawnSync` (not `node:child_process.spawnSync`) so the
 * stagers are unaffected by test files that globally stub
 * `child_process` via `mock.module`.
 *
 * WARNING: these tests mutate the user's real clipboard. Running them
 * locally will overwrite whatever is currently copied.
 */

import { afterAll, beforeEach, describe, expect, it } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { readClipboardImage, readPngDimensions } from '../clipboard-image';

// --- Test fixtures ----------------------------------------------------

/**
 * A valid 1×1 transparent PNG. Used as the clipboard payload for the
 * round-trip tests. Decoded from a well-known base64 constant so we
 * don't depend on any image library at test time.
 */
const TINY_PNG_BASE64 =
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==';
const TINY_PNG = Buffer.from(TINY_PNG_BASE64, 'base64');

// Shared temp dir for PNG fixtures written to disk so platform stagers
// can reference them by path.
let tempDir: string;

beforeEach(() => {
  tempDir = mkdtempSync(join(tmpdir(), 'kiro-clip-test-'));
});

afterAll(() => {
  if (tempDir) rmSync(tempDir, { recursive: true, force: true });
});

function writeFixturePng(name: string, bytes: Buffer): string {
  const p = join(tempDir, name);
  writeFileSync(p, bytes);
  return p;
}

/** Run a command to completion via Bun's native spawn, asserting success. */
function runOrThrow(cmd: string[], opts: { input?: string | Buffer } = {}) {
  const r = Bun.spawnSync(cmd, {
    stdout: 'pipe',
    stderr: 'pipe',
    ...(opts.input != null ? { stdin: Buffer.from(opts.input) } : {}),
  });
  if (r.exitCode !== 0) {
    const stderr = Buffer.from(r.stderr).toString('utf-8');
    throw new Error(
      `${cmd.join(' ')} failed (exit ${r.exitCode}): ${stderr || '<no stderr>'}`
    );
  }
  return r;
}

function binaryAvailable(bin: string): boolean {
  try {
    const which = process.platform === 'win32' ? 'where' : 'which';
    const r = Bun.spawnSync([which, bin], {
      stdout: 'ignore',
      stderr: 'ignore',
    });
    return r.exitCode === 0;
  } catch {
    return false;
  }
}

// --- Pure PNG header parsing (platform-agnostic) ---------------------

describe('readPngDimensions', () => {
  it('decodes width and height from the IHDR of a real 1x1 PNG', () => {
    expect(readPngDimensions(TINY_PNG)).toEqual({ width: 1, height: 1 });
  });

  it('returns null for a buffer too short to contain an IHDR', () => {
    expect(readPngDimensions(Buffer.alloc(10))).toBeNull();
  });

  it('returns null when the PNG signature is wrong', () => {
    const bad = Buffer.from(TINY_PNG);
    bad[0] = 0x00; // Corrupt the signature magic byte.
    expect(readPngDimensions(bad)).toBeNull();
  });

  it('returns null for an empty buffer', () => {
    expect(readPngDimensions(Buffer.alloc(0))).toBeNull();
  });
});

// --- macOS round-trip (osascript) ------------------------------------

describe.if(process.platform === 'darwin')(
  'readClipboardImage on macOS',
  () => {
    /** Put a PNG on the clipboard via AppleScript («class PNGf»). */
    function stagePngOnClipboard(pngPath: string): void {
      const script = `set the clipboard to (read (POSIX file "${pngPath}") as «class PNGf»)`;
      runOrThrow(['osascript', '-e', script]);
    }

    /** Overwrite the clipboard with plain text (guarantees no image). */
    function stageTextOnClipboard(text: string): void {
      runOrThrow(['pbcopy'], { input: text });
    }

    it('reads back the PNG bytes staged via osascript', () => {
      const path = writeFixturePng('tiny.png', TINY_PNG);
      stagePngOnClipboard(path);

      const result = readClipboardImage();

      expect(result.ok).toBe(true);
      if (!result.ok) return;
      expect(result.image.mimeType).toBe('image/png');
      expect(result.image.width).toBe(1);
      expect(result.image.height).toBe(1);
      // Round-trip the base64 and verify the PNG header is intact. We
      // don't compare the full byte sequence because AppleScript may
      // re-encode PNGs through QuickLook, yielding byte-different but
      // semantically identical output.
      const roundTripped = Buffer.from(result.image.data, 'base64');
      expect(readPngDimensions(roundTripped)).toEqual({ width: 1, height: 1 });
      expect(result.image.sizeBytes).toBe(roundTripped.length);
    });

    it('returns "No image in clipboard" when the clipboard holds text', () => {
      stageTextOnClipboard('not an image at all');
      const result = readClipboardImage();
      expect(result.ok).toBe(false);
      if (result.ok) return;
      expect(result.error.message).toBe('No image in clipboard');
    });
  }
);

// --- Linux round-trip (wl-copy / xclip) ------------------------------
//
// Linux clipboards require a running display server (X11 or Wayland).
// Headless GHA runners have neither, so we detect that case and skip.

const linuxHasClipboard =
  process.platform === 'linux' &&
  ((process.env.WAYLAND_DISPLAY != null && binaryAvailable('wl-copy')) ||
    (process.env.DISPLAY != null && binaryAvailable('xclip')));

describe.if(linuxHasClipboard)('readClipboardImage on Linux', () => {
  /**
   * Put PNG bytes on the clipboard using whichever tool the user's
   * session supports.
   */
  function stagePngOnClipboard(pngPath: string): void {
    const bytes = readFileSync(pngPath);
    if (process.env.WAYLAND_DISPLAY && binaryAvailable('wl-copy')) {
      runOrThrow(['wl-copy', '--type', 'image/png'], { input: bytes });
      return;
    }
    if (process.env.DISPLAY && binaryAvailable('xclip')) {
      runOrThrow(
        ['xclip', '-selection', 'clipboard', '-t', 'image/png', '-i'],
        { input: bytes }
      );
      return;
    }
    throw new Error('no supported clipboard tool');
  }

  it('reads back the PNG bytes staged via the Linux clipboard tool', () => {
    const path = writeFixturePng('tiny.png', TINY_PNG);
    stagePngOnClipboard(path);

    const result = readClipboardImage();

    expect(result.ok).toBe(true);
    if (!result.ok) return;
    expect(result.image.mimeType).toBe('image/png');
    expect(result.image.width).toBe(1);
    expect(result.image.height).toBe(1);
    const roundTripped = Buffer.from(result.image.data, 'base64');
    expect(readPngDimensions(roundTripped)).toEqual({ width: 1, height: 1 });
  });
});

// --- Windows round-trip (PowerShell) ---------------------------------

describe.if(process.platform === 'win32')(
  'readClipboardImage on Windows',
  () => {
    /**
     * Put an image on the clipboard via PowerShell
     * `System.Windows.Forms.Clipboard.SetImage`. Requires STA just like
     * the reader does.
     */
    function stagePngOnClipboard(pngPath: string): void {
      const ps = [
        'Add-Type -AssemblyName System.Windows.Forms',
        'Add-Type -AssemblyName System.Drawing',
        `$img = [System.Drawing.Image]::FromFile('${pngPath.replace(/\\/g, '\\\\')}')`,
        '[System.Windows.Forms.Clipboard]::SetImage($img)',
        '$img.Dispose()',
      ].join('; ');
      runOrThrow(['powershell', '-NoProfile', '-STA', '-Command', ps]);
    }

    /** Overwrite the clipboard with plain text (guarantees no image). */
    function stageTextOnClipboard(text: string): void {
      runOrThrow(
        ['powershell', '-NoProfile', '-Command', 'Set-Clipboard -Value $input'],
        { input: text }
      );
    }

    it('reads back the PNG bytes staged via PowerShell', () => {
      const path = writeFixturePng('tiny.png', TINY_PNG);
      stagePngOnClipboard(path);

      const result = readClipboardImage();

      expect(result.ok).toBe(true);
      if (!result.ok) return;
      expect(result.image.mimeType).toBe('image/png');
      expect(result.image.width).toBe(1);
      expect(result.image.height).toBe(1);
      const roundTripped = Buffer.from(result.image.data, 'base64');
      expect(readPngDimensions(roundTripped)).toEqual({ width: 1, height: 1 });
    });

    it('returns "No image in clipboard" when the clipboard holds text', () => {
      stageTextOnClipboard('not an image at all');
      const result = readClipboardImage();
      expect(result.ok).toBe(false);
      if (result.ok) return;
      expect(result.error.message).toBe('No image in clipboard');
    });
  }
);
