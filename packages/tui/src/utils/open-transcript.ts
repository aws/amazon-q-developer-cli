import { mkdtempSync, writeFileSync, unlinkSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import { spawnSync } from 'child_process';
import { serializeConversationToMarkdown } from './serialize-conversation.js';
import { executeShellEscapeTTY, restoreTerminalModes } from './shell-escape.js';

export function openTranscriptInPager(
  messages: Array<{ role: string; content: string }>,
  preRendered?: string,
  format: 'md' | 'txt' | 'json' = 'md'
): void {
  if (!messages.length) return;

  const content =
    preRendered ?? serializeConversationToMarkdown(messages, 'markdown');
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
        const result = spawnSync('notepad', [tempFile], { stdio: 'inherit' });
        restoreTerminalModes();
        if (result.error) {
          process.stderr.write(
            `Could not open transcript: ${result.error.message}\n`
          );
        }
      }
    } else {
      const pager = process.env.PAGER || 'less';
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
