/**
 * Cross-platform clipboard image reader.
 *
 * Shells out to platform-native tools to extract an image from the system
 * clipboard, normalizes it to PNG, and returns base64-encoded bytes plus
 * dimensions/size metadata.
 *
 * Strategy per platform:
 *   macOS   → osascript (saves «class PNGf» to a temp file via AppleScript)
 *   Windows → powershell (System.Windows.Forms.Clipboard.GetImage())
 *   Linux   → wl-paste (Wayland) → xclip → xsel (each with image/png MIME)
 *
 * Unlike the Rust backend (which links arboard), this runs entirely in
 * Node-land so it works in KAS mode without additional native deps.
 *
 * Note: We use `Bun.spawnSync` rather than `node:child_process.spawnSync`
 * so the reader is unaffected by test files that stub `child_process`
 * globally via `mock.module`. The TUI always runs under Bun in both
 * production and tests.
 */

import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

export interface ClipboardImage {
  /** Base64-encoded PNG bytes. */
  data: string;
  /** Always `image/png` — platform tools normalize to PNG. */
  mimeType: 'image/png';
  width: number;
  height: number;
  sizeBytes: number;
}

export interface ClipboardReadError {
  /** Human-readable error message surfaced to the user. */
  message: string;
}

export type ClipboardReadResult =
  | { ok: true; image: ClipboardImage }
  | { ok: false; error: ClipboardReadError };

/**
 * Decode the width/height from the IHDR chunk of a PNG file.
 *
 * PNG layout:
 *   0..7   : signature (\x89PNG\r\n\x1a\n)
 *   8..11  : IHDR chunk length (always 13)
 *   12..15 : "IHDR"
 *   16..19 : width  (big-endian u32)
 *   20..23 : height (big-endian u32)
 *
 * Returns null if the buffer doesn't look like a PNG.
 */
export function readPngDimensions(
  buf: Buffer
): { width: number; height: number } | null {
  if (buf.length < 24) return null;
  // Signature check — magic bytes that identify a PNG file.
  const sig = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a];
  for (let i = 0; i < sig.length; i++) {
    if (buf[i] !== sig[i]) return null;
  }
  const width = buf.readUInt32BE(16);
  const height = buf.readUInt32BE(20);
  return { width, height };
}

/**
 * Run a command with a timeout, returning `{ status, stdout }` on success
 * or `null` if the binary is not installed / timed out / errored.
 *
 * We keep stdout as a Buffer (not a string) so binary PNG output from
 * xclip / wl-paste doesn't get mangled by string decoding.
 *
 * Uses `Bun.spawnSync` so tests that globally mock `child_process` don't
 * neuter the clipboard reader when the full test suite is run.
 */
function runCapture(
  bin: string,
  args: string[]
): { status: number; stdout: Buffer; stderr: string } | null {
  try {
    const result = Bun.spawnSync([bin, ...args], {
      stdin: 'ignore',
      stdout: 'pipe',
      stderr: 'pipe',
      timeout: 5000,
    });
    return {
      status: result.exitCode ?? 1,
      stdout: Buffer.from(result.stdout),
      stderr: Buffer.from(result.stderr).toString('utf-8'),
    };
  } catch {
    return null;
  }
}

/**
 * macOS — use AppleScript via `osascript` to save PNG clipboard data to
 * a temp file. AppleScript's «class PNGf» reads the clipboard as PNG if
 * the clipboard contains image data (works for screenshots, Preview
 * copies, etc.). Returns the PNG bytes or null if the clipboard has no
 * image content.
 */
function readMacClipboardPng(): Buffer | null {
  // Use a dedicated temp dir so we clean up exactly one file + dir.
  const dir = mkdtempSync(join(tmpdir(), 'kiro-clip-'));
  const pngPath = join(dir, 'clipboard.png');
  try {
    const script = [
      'try',
      '  set pngData to the clipboard as «class PNGf»',
      `  set fp to open for access POSIX file "${pngPath}" with write permission`,
      '  set eof of fp to 0',
      '  write pngData to fp',
      '  close access fp',
      '  return "ok"',
      'on error errMsg',
      '  try',
      '    close access fp',
      '  end try',
      '  return "error: " & errMsg',
      'end try',
    ].join('\n');

    const result = runCapture('osascript', ['-e', script]);
    if (!result || result.status !== 0) return null;
    const output = result.stdout.toString('utf-8').trim();
    if (!output.startsWith('ok')) return null;
    return readFileSync(pngPath);
  } catch {
    return null;
  } finally {
    try {
      rmSync(dir, { recursive: true, force: true });
    } catch {
      // best-effort cleanup
    }
  }
}

/**
 * Linux — try Wayland (wl-paste) first, then X11 tools (xclip, xsel).
 * Each tool is asked for the `image/png` MIME type. Returns PNG bytes on
 * first success, or null if no tool is installed or none has image data.
 *
 * Note: xsel doesn't have native MIME type support, so we only use it
 * as a last-resort for PNG-formatted clipboards (rare but possible).
 */
function readLinuxClipboardPng(): Buffer | null {
  const candidates: Array<{ bin: string; args: string[] }> = [];

  if (process.env.WAYLAND_DISPLAY) {
    candidates.push({ bin: 'wl-paste', args: ['--type', 'image/png'] });
  }
  candidates.push({
    bin: 'xclip',
    args: ['-selection', 'clipboard', '-t', 'image/png', '-o'],
  });

  for (const { bin, args } of candidates) {
    const result = runCapture(bin, args);
    if (!result) continue;
    if (result.status === 0 && result.stdout.length > 0) {
      // Validate we actually got a PNG, not an error message on stdout.
      if (readPngDimensions(result.stdout)) return result.stdout;
    }
  }

  return null;
}

/**
 * Windows — use PowerShell's System.Windows.Forms.Clipboard.GetImage()
 * and save to a temp file as PNG. Empty output means no image in clipboard.
 */
function readWindowsClipboardPng(): Buffer | null {
  const dir = mkdtempSync(join(tmpdir(), 'kiro-clip-'));
  const pngPath = join(dir, 'clipboard.png');
  try {
    // PowerShell script: load Windows Forms + Drawing, grab clipboard
    // image, save as PNG. Emits "ok" on success, "empty" if nothing, or
    // "error: ..." for anything else.
    const script = `
      Add-Type -AssemblyName System.Windows.Forms
      Add-Type -AssemblyName System.Drawing
      try {
        $img = [System.Windows.Forms.Clipboard]::GetImage()
        if ($img -eq $null) { Write-Output 'empty'; exit 0 }
        $img.Save('${pngPath.replace(/\\/g, '\\\\')}', [System.Drawing.Imaging.ImageFormat]::Png)
        Write-Output 'ok'
      } catch {
        Write-Output ('error: ' + $_.Exception.Message)
      }
    `;
    const result = runCapture('powershell', [
      '-NoProfile',
      '-STA',
      '-Command',
      script,
    ]);
    if (!result || result.status !== 0) return null;
    const output = result.stdout.toString('utf-8').trim();
    if (output !== 'ok') return null;
    return readFileSync(pngPath);
  } catch {
    return null;
  } finally {
    try {
      rmSync(dir, { recursive: true, force: true });
    } catch {
      // best-effort cleanup
    }
  }
}

/**
 * Platform-dispatch for clipboard reading. Exported as a separate function
 * so tests can mock the raw byte reader independently from the encoding
 * + metadata-extraction logic in `readClipboardImage()`.
 */
export function readClipboardImageBytes(): Buffer | null {
  switch (process.platform) {
    case 'darwin':
      return readMacClipboardPng();
    case 'win32':
      return readWindowsClipboardPng();
    case 'linux':
      return readLinuxClipboardPng();
    default:
      return null;
  }
}

/**
 * Read an image from the system clipboard.
 *
 * Returns `{ ok: true, image }` with base64-encoded PNG data and
 * dimensions on success, or `{ ok: false, error }` with a user-visible
 * message on failure. Failure modes include: no image in clipboard,
 * no clipboard tool installed, or platform not supported.
 */
export function readClipboardImage(): ClipboardReadResult {
  if (
    process.platform !== 'darwin' &&
    process.platform !== 'linux' &&
    process.platform !== 'win32'
  ) {
    return {
      ok: false,
      error: {
        message: `Clipboard image paste is not supported on ${process.platform}`,
      },
    };
  }

  let png: Buffer | null;
  try {
    png = readClipboardImageBytes();
  } catch (e) {
    return {
      ok: false,
      error: {
        message: `Failed to read clipboard: ${e instanceof Error ? e.message : String(e)}`,
      },
    };
  }

  if (!png || png.length === 0) {
    return {
      ok: false,
      error: { message: 'No image in clipboard' },
    };
  }

  const dims = readPngDimensions(png);
  if (!dims) {
    return {
      ok: false,
      error: { message: 'Clipboard data is not a valid PNG image' },
    };
  }

  return {
    ok: true,
    image: {
      data: png.toString('base64'),
      mimeType: 'image/png',
      width: dims.width,
      height: dims.height,
      sizeBytes: png.length,
    },
  };
}
