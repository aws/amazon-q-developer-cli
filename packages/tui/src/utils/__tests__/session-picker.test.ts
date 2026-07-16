import { describe, it, expect } from 'bun:test';
import { formatMergedEntry, formatSessionState } from '../session-picker';
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

  it('tags a cloud-sandbox session with the cloud Environment when shown', () => {
    const result = formatMergedEntry(
      baseEntry({ title: 'alpha task', executionTarget: 'cloud-sandbox' }),
      200,
      true
    );
    expect(result).toContain('| cloud');
  });

  it('tags a local session as local when the Environment column is shown', () => {
    const result = formatMergedEntry(
      baseEntry({ title: 'beta task', executionTarget: undefined }),
      200,
      true
    );
    expect(result).toContain('| local');
  });

  it('shows no Environment tag when not requested (dark-safe default)', () => {
    const cloud = formatMergedEntry(
      baseEntry({ title: 'alpha task', executionTarget: 'cloud-sandbox' }),
      200
    );
    const local = formatMergedEntry(
      baseEntry({ title: 'beta task', executionTarget: undefined }),
      200
    );
    expect(cloud).not.toContain('| cloud');
    expect(cloud).not.toContain('| local');
    expect(local).not.toContain('| cloud');
    expect(local).not.toContain('| local');
  });
});

describe('formatMergedEntry state column', () => {
  it('shows the mapped state when the Environment column is shown', () => {
    const result = formatMergedEntry(
      baseEntry({ executionTarget: 'cloud-sandbox', status: 'in_progress' }),
      200,
      true
    );
    expect(result).toContain('| working');
  });

  it('hides the state when the Environment column is not shown (dark-safe)', () => {
    const result = formatMergedEntry(
      baseEntry({ executionTarget: 'cloud-sandbox', status: 'in_progress' }),
      200,
      false
    );
    expect(result).not.toContain('working');
  });

  it('omits the state for a row with no status', () => {
    const result = formatMergedEntry(
      baseEntry({ executionTarget: 'cloud-sandbox', status: undefined }),
      200,
      true
    );
    expect(result).toContain('| cloud');
    expect(result).not.toContain('| working');
  });
});

describe('formatSessionState', () => {
  it('maps coarse statuses to short words', () => {
    expect(formatSessionState('in_progress')).toBe('working');
    expect(formatSessionState('waiting_on_user')).toBe('waiting');
    expect(formatSessionState('completed')).toBe('done');
  });

  it('passes through idle/failed/provisioning and unknown values', () => {
    expect(formatSessionState('idle')).toBe('idle');
    expect(formatSessionState('failed')).toBe('failed');
    expect(formatSessionState('provisioning')).toBe('provisioning');
    expect(formatSessionState('future_status')).toBe('future_status');
  });
});
