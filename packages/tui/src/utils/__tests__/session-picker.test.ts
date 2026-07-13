import { describe, it, expect } from 'bun:test';
import { formatMergedEntry } from '../session-picker';
import type { SessionEntry } from '../list-all-sessions-cli';

const baseEntry = (overrides: Partial<SessionEntry>): SessionEntry => ({
  sessionId: 'sess-aaaa-bbbb',
  source: 'v3',
  title: 'a session',
  updatedAt: new Date(Date.now() - 30 * 1000).toISOString(),
  messageCount: 3,
  ...overrides,
});

describe('formatMergedEntry', () => {
  it('renders title and message count on a single line', () => {
    const result = formatMergedEntry(
      baseEntry({ title: 'fix login bug' }),
      200
    );
    expect(result).toContain('fix login bug');
    expect(result).toContain('3 msgs');
    expect(result).not.toContain('\n');
  });

  it('shows "(no title)" when title is empty', () => {
    const result = formatMergedEntry(baseEntry({ title: '' }), 200);
    expect(result).toContain('(no title)');
  });

  it('strips raw newlines from a multi-line title', () => {
    // The cross-engine resume picker redraws by counting menu lines and
    // moving the cursor up. A title containing real `\n`s pushes items
    // off-screen and the next render writes over the wrong rows.
    const result = formatMergedEntry(
      baseEntry({ title: 'line one\nline two\nline three' }),
      200
    );
    expect(result).not.toContain('\n');
    expect(result).not.toContain('\r');
    expect(result).toContain('line one');
    expect(result).toContain('line two');
  });

  it('strips \\r\\n from a Windows-style multi-line title', () => {
    const result = formatMergedEntry(baseEntry({ title: 'a\r\nb' }), 200);
    expect(result).not.toContain('\n');
    expect(result).not.toContain('\r');
  });

  it('truncates the line to fit maxWidth', () => {
    const result = formatMergedEntry(baseEntry({ title: 'x'.repeat(200) }), 40);
    // maxLen = 40 - 4 = 36, truncated to 33 + "..."
    expect(result.length).toBeLessThanOrEqual(36);
    expect(result).toEndWith('...');
  });

  it('tags a cloud-sandbox session with a WHERE indicator', () => {
    const result = formatMergedEntry(
      baseEntry({ title: 'remote task', executionTarget: 'cloud-sandbox' }),
      200
    );
    expect(result).toContain('cloud');
  });

  it('shows no WHERE tag for a local session (executionTarget absent — dark-safe)', () => {
    const result = formatMergedEntry(
      baseEntry({ title: 'local task', executionTarget: undefined }),
      200
    );
    expect(result).not.toContain('cloud');
  });
});
