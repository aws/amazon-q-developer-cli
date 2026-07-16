import { describe, test, expect } from 'vitest';
import { formatCloudStartupChecklist } from '../cloud-startup-checklist.js';

// chalk wraps output in escape codes — strip them to assert visible text.
// eslint-disable-next-line no-control-regex
const ansiStrip = (s: string) => s.replace(/\x1B\[[0-9;]*m/g, '');
const visible = (rows: string[]) => rows.map(ansiStrip);
const G = { check: '✓', cross: '✗', spinner: '⠋', ellipsis: '…' };

describe('formatCloudStartupChecklist', () => {
  test('shows a spinner on the first pending step, hiding later steps', () => {
    const rows = visible(
      formatCloudStartupChecklist(
        { connected: false, sessionCreated: false },
        G
      )
    );
    // Only the in-progress line shows; "Cloud session created" is not reached.
    expect(rows).toEqual(['  ⠋ Connecting to kiro.dev…']);
  });

  test('ticks a completed step to a ✓ line and advances the spinner', () => {
    const rows = visible(
      formatCloudStartupChecklist({ connected: true, sessionCreated: false }, G)
    );
    expect(rows).toEqual([
      '  ✓ Connected to kiro.dev',
      '  ⠋ Creating cloud session…',
    ]);
  });

  test('all done: ✓ lines + provider line + /repo hint + upload guidance', () => {
    const rows = visible(
      formatCloudStartupChecklist(
        { connected: true, sessionCreated: true, provider: 'GitHub' },
        G
      )
    );
    expect(rows.slice(0, 4)).toEqual([
      '  ✓ Connected to kiro.dev',
      '  ✓ Connected to GitHub',
      '  ✓ Cloud session created',
      '  /repo to select (optional)',
    ]);
    // A blank spacer then the upload-setup guidance paragraph.
    expect(rows[4]).toBe('');
    expect(rows[5]).toContain("doesn't have your local setup yet");
    expect(rows[5]).toContain('kiro.dev/config/upload');
  });

  test('a known repo count renders "✓ N repositories found, /repo to select"', () => {
    const rows = visible(
      formatCloudStartupChecklist(
        {
          connected: true,
          sessionCreated: true,
          provider: 'GitHub',
          repoCount: 3,
        },
        G
      )
    );
    expect(rows[3]).toBe(
      '  ✓ 3 repositories found, /repo to select (optional)'
    );
  });

  test('a repo count of 1 uses the singular noun', () => {
    const rows = visible(
      formatCloudStartupChecklist(
        { connected: true, sessionCreated: true, repoCount: 1 },
        G
      )
    );
    expect(rows.join('\n')).toContain('1 repository found');
  });

  test('provider line is omitted until a provider is known', () => {
    const rows = visible(
      formatCloudStartupChecklist({ connected: true, sessionCreated: true }, G)
    );
    expect(rows.slice(0, 3)).toEqual([
      '  ✓ Connected to kiro.dev',
      '  ✓ Cloud session created',
      '  /repo to select (optional)',
    ]);
  });

  test('lists multiple connected providers on one line', () => {
    const rows = visible(
      formatCloudStartupChecklist(
        { connected: true, sessionCreated: true, provider: 'GitHub, GitFarm' },
        G
      )
    );
    expect(rows[1]).toBe('  ✓ Connected to GitHub, GitFarm');
  });

  test('omits the count when it is not yet known (bare /repo hint)', () => {
    const rows = visible(
      formatCloudStartupChecklist({ connected: true, sessionCreated: true }, G)
    );
    // No count row when repoCount is undefined; the bare hint is used instead.
    expect(rows.join('\n')).not.toMatch(/repositories found/);
    expect(rows).toContain('  /repo to select (optional)');
  });

  test('failed create shows a ✗ row, no spinner, no /repo hint', () => {
    const rows = visible(
      formatCloudStartupChecklist(
        { connected: true, sessionCreated: false, sessionFailed: true },
        G
      )
    );
    expect(rows).toEqual([
      '  ✓ Connected to kiro.dev',
      '  ✗ Cloud session failed',
    ]);
    expect(rows.join('\n')).not.toContain('⠋');
    expect(rows.join('\n')).not.toMatch(/\/repo/);
  });

  test('ASCII glyphs leave no Unicode in any rendered state', () => {
    const ascii = { check: '+', cross: 'x', spinner: '-', ellipsis: '...' };
    const states = [
      { connected: false, sessionCreated: false },
      { connected: true, sessionCreated: false },
      {
        connected: true,
        sessionCreated: true,
        provider: 'GitHub',
        repoCount: 3,
      },
      { connected: true, sessionCreated: false, sessionFailed: true },
    ];
    for (const state of states) {
      const out = visible(formatCloudStartupChecklist(state, ascii)).join('\n');
      // eslint-disable-next-line no-control-regex
      expect(out).toMatch(/^[\x00-\x7F]*$/);
    }
  });
});
