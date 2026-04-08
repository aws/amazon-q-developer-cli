import { mkdtempSync, writeFileSync, unlinkSync } from 'fs';
import { join } from 'path';
import { tmpdir } from 'os';
import { serializeConversationToMarkdown } from './serialize-conversation.js';
import { executeShellEscapeTTY } from './shell-escape.js';

export function openTranscriptInPager(
  messages: Array<{ role: string; content: string }>
): void {
  if (!messages.length) return;

  const markdown = serializeConversationToMarkdown(messages);
  const tempDir = mkdtempSync(join(tmpdir(), 'kiro-raw-'));
  const tempFile = join(tempDir, 'conversation.md');

  try {
    writeFileSync(tempFile, markdown);
    const pager = process.env.PAGER || 'less';
    const quotedPath = `'${tempFile.replace(/'/g, "'\\''")}'`;
    executeShellEscapeTTY(`${pager} ${quotedPath}`);
  } finally {
    try { unlinkSync(tempFile); } catch { /* ignore */ }
  }
}
