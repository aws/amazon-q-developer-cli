import {
  mkdtempSync as realMkdtempSync,
  writeFileSync as realWriteFileSync,
  unlinkSync as realUnlinkSync,
} from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import { spawnSync as realSpawnSync } from 'child_process';
import { serializeConversationToMarkdown } from './serialize-conversation.js';
import {
  executeShellEscapeTTY as realExecuteShellEscapeTTY,
  restoreTerminalModes as realRestoreTerminalModes,
} from './shell-escape.js';
import { system32Path } from './windows-paths.js';

/**
 * Injectable dependencies for {@link openTranscriptInPager}. Every field
 * defaults to the real implementation, so production callers pass nothing.
 * Tests inject fakes directly — keeping the test hermetic (no real temp files,
 * pager, or terminal mutation) without process-global module mocking, which
 * leaks across bun test files.
 */
export interface OpenTranscriptDeps {
  spawnSync?: typeof realSpawnSync;
  mkdtempSync?: typeof realMkdtempSync;
  writeFileSync?: typeof realWriteFileSync;
  unlinkSync?: typeof realUnlinkSync;
  executeShellEscapeTTY?: typeof realExecuteShellEscapeTTY;
  restoreTerminalModes?: typeof realRestoreTerminalModes;
}

export function openTranscriptInPager(
  messages: Array<{ role: string; content: string }>,
  preRendered?: string,
  format: 'md' | 'txt' | 'json' = 'md',
  deps: OpenTranscriptDeps = {}
): void {
  if (!messages.length) return;

  const spawnSync = deps.spawnSync ?? realSpawnSync;
  const mkdtempSync = deps.mkdtempSync ?? realMkdtempSync;
  const writeFileSync = deps.writeFileSync ?? realWriteFileSync;
  const unlinkSync = deps.unlinkSync ?? realUnlinkSync;
  const executeShellEscapeTTY =
    deps.executeShellEscapeTTY ?? realExecuteShellEscapeTTY;
  const restoreTerminalModes =
    deps.restoreTerminalModes ?? realRestoreTerminalModes;

  const content = preRendered ?? serializeConversationToMarkdown(messages);
  const tempDir = mkdtempSync(join(tmpdir(), 'kiro-raw-'));
  const tempFile = join(tempDir, `conversation.${format}`);

  try {
    writeFileSync(tempFile, content);

    if (process.platform === 'win32') {
      const pager = process.env.PAGER;
      if (pager) {
        const { error } = executeShellEscapeTTY(`${pager} "${tempFile}"`);
        if (error) {
          process.stderr.write(
            `Could not open transcript with PAGER="${pager}": ${error}\n`
          );
        }
      } else {
        // Launch Notepad by its absolute System32 path with shell:false so a
        // planted `.\notepad.exe` cannot hijack it (CWE-426); see system32Path.
        const result = spawnSync(system32Path('notepad.exe'), [tempFile], {
          stdio: 'inherit',
          shell: false,
        });
        restoreTerminalModes();
        if (result.error) {
          process.stderr.write(
            `Could not open transcript: ${result.error.message}\n`
          );
        }
      }
    } else {
      // -F: auto-exit if content fits on one screen (avoids opening less for
      // tiny transcripts); -X: don't init/deinit terminal so quitting leaves
      // the transcript visible in scrollback instead of wiping it. $PAGER
      // takes precedence so users with a configured pager keep their setup.
      const pager = process.env.PAGER || 'less -F -X';
      const quotedPath = `'${tempFile.replace(/'/g, "'\\''")}'`;
      // Start at the bottom so the most recent messages are visible first.
      // +G is understood by less and most less-compatible pagers.
      const startAtEnd = pager.startsWith('less') ? '+G ' : '';
      const { error } = executeShellEscapeTTY(
        `${pager} ${startAtEnd}${quotedPath}`
      );
      if (error) {
        process.stderr.write(
          `Could not open transcript with pager "${pager}": ${error}\n`
        );
      }
    }
  } finally {
    try {
      unlinkSync(tempFile);
    } catch {
      /* ignore */
    }
  }
}
