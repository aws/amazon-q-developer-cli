import { writeFileSync, readFileSync, unlinkSync, mkdtempSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import { executeShellEscapeTTY } from './shell-escape.js';

export interface EditorOptions {
  prefix: string;
  filename: string;
  initialContent?: string;
  /** Return an error message if content is invalid, or undefined if valid. */
  validate?: (content: string) => string | undefined;
}

export type EditorResult =
  | { ok: true; content: string }
  | { ok: false; error: string };

export function openEditorSync(opts: EditorOptions): EditorResult {
  const tempDir = mkdtempSync(join(tmpdir(), opts.prefix));
  const tempFile = join(tempDir, opts.filename);
  try {
    writeFileSync(tempFile, opts.initialContent ?? '');
    const editor = process.env.VISUAL || process.env.EDITOR || 'vi';
    const quotedPath = `'${tempFile.replace(/'/g, "'\\''")}'`;
    const { exitCode, error } = executeShellEscapeTTY(
      `${editor} ${quotedPath}`
    );
    if (exitCode !== 0) {
      return {
        ok: false,
        error: error ?? `Editor exited with code ${exitCode}`,
      };
    }
    const content = readFileSync(tempFile, 'utf-8').trim();
    const validationError = opts.validate?.(content);
    if (validationError) {
      return { ok: false, error: validationError };
    }
    return { ok: true, content };
  } catch (err) {
    return {
      ok: false,
      error: err instanceof Error ? err.message : 'Failed to open editor',
    };
  } finally {
    try {
      unlinkSync(tempFile);
    } catch {
      /* ignore cleanup errors */
    }
  }
}
