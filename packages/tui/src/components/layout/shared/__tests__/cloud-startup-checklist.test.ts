import { describe, test, expect } from 'vitest';
import { formatCloudStartupChecklist } from '../cloud-startup-checklist.js';

// chalk wraps output in escape codes — strip them to assert visible text.
// eslint-disable-next-line no-control-regex
const ansiStrip = (s: string) => s.replace(/\x1B\[[0-9;]*m/g, '');
const visible = (rows: string[]) => rows.map(ansiStrip);
const G = { check: '✓', cross: '✗', spinner: '⠋', ellipsis: '…' };

// The cloud-config guidance paragraph closes the checklist in every non-failed
// state: a blank spacer row then the dim paragraph (link in host-relative form).
const HINT =
  "  Your cloud workspace doesn't have your local setup by default. Go to " +
  'app.kiro.dev/settings/cloud-config to bring your agents, MCP servers, hooks, ' +
  'and steering from ~/.kiro/ (home directory) to the cloud.';
const withHint = (...rows: string[]) => [...rows, '', HINT];

describe('formatCloudStartupChecklist', () => {
  test('shows a spinner on the first pending step, hiding later steps', () => {
    const rows = visible(
      formatCloudStartupChecklist(
        { connected: false, sessionCreated: false },
        G
      )
    );
    // Only the in-progress line shows; "Cloud session created" is not reached.
    // The config-hint paragraph closes every non-failed state, connecting too.
    expect(rows).toEqual(withHint('  ⠋ Connecting to kiro.dev…'));
  });

  test('ticks a completed step to a ✓ line and advances the spinner', () => {
    const rows = visible(
      formatCloudStartupChecklist({ connected: true, sessionCreated: false }, G)
    );
    expect(rows).toEqual(
      withHint('  ✓ Connected to kiro.dev', '  ⠋ Creating cloud session…')
    );
  });

  test('all done: ✓ lines + provider line + /repo hint, then config guidance', () => {
    const rows = visible(
      formatCloudStartupChecklist(
        { connected: true, sessionCreated: true, provider: 'GitHub' },
        G
      )
    );
    // The /repo hint is followed by the cloud-config guidance paragraph.
    expect(rows).toEqual(
      withHint(
        '  ✓ Connected to kiro.dev',
        '  ✓ Connected to GitHub',
        '  ✓ Cloud session created',
        '  /repo to select (optional)'
      )
    );
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

  test('resumed session shows resume wording on the pending step', () => {
    const rows = visible(
      formatCloudStartupChecklist(
        { connected: true, sessionCreated: false, resumed: true },
        G
      )
    );
    expect(rows).toEqual(
      withHint('  ✓ Connected to kiro.dev', '  ⠋ Resuming cloud session…')
    );
  });

  test('resumed session ticks to "✓ Cloud session resumed" when done', () => {
    const rows = visible(
      formatCloudStartupChecklist(
        {
          connected: true,
          sessionCreated: true,
          resumed: true,
          provider: 'GitHub',
        },
        G
      )
    );
    expect(rows).toEqual(
      withHint(
        '  ✓ Connected to kiro.dev',
        '  ✓ Connected to GitHub',
        '  ✓ Cloud session resumed',
        '  /repo to select (optional)'
      )
    );
  });

  test('a failed resume keeps the generic "Cloud session failed" row', () => {
    const rows = visible(
      formatCloudStartupChecklist(
        {
          connected: true,
          sessionCreated: false,
          sessionFailed: true,
          resumed: true,
        },
        G
      )
    );
    expect(rows[1]).toBe('  ✗ Cloud session failed');
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

  test('config-hint paragraph closes the connecting state', () => {
    const rows = visible(
      formatCloudStartupChecklist(
        { connected: false, sessionCreated: false },
        G
      )
    );
    // Blank spacer then the paragraph, ending the row list.
    expect(rows.slice(-2)).toEqual(['', HINT]);
  });

  test('config-hint paragraph closes the fully-bootstrapped state', () => {
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
    expect(rows.slice(-2)).toEqual(['', HINT]);
  });

  test('config-hint links the cloud-config settings page', () => {
    const rows = formatCloudStartupChecklist(
      { connected: true, sessionCreated: true },
      G
    );
    // The link is present in the raw (still-colored) output too.
    expect(visible(rows).join('\n')).toContain(
      'app.kiro.dev/settings/cloud-config'
    );
  });

  test('config-hint is suppressed on a failed session', () => {
    const rows = visible(
      formatCloudStartupChecklist(
        { connected: true, sessionCreated: false, sessionFailed: true },
        G
      )
    );
    expect(rows.join('\n')).not.toContain('cloud-config');
    expect(rows.join('\n')).not.toContain("doesn't have your local setup");
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
      { connected: true, sessionCreated: true, resumed: true },
    ];
    for (const state of states) {
      const out = visible(formatCloudStartupChecklist(state, ascii)).join('\n');
      // eslint-disable-next-line no-control-regex
      expect(out).toMatch(/^[\x00-\x7F]*$/);
    }
  });
});
