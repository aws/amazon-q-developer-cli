/**
 * Unit tests for the `/settings` panel navigation model.
 *
 * These exist to catch the class of regression where a menu entry becomes
 * unreachable from the visual panel — most notably the **Terminal → Interrupt
 * behaviour** path, which once silently disappeared because selecting Terminal
 * jumped straight into the newlines setup. The tests assert the full menu graph
 * is reachable and that every leaf maps to the correct action.
 */

import { describe, it, expect } from 'bun:test';
import {
  TOP_ITEMS,
  TERMINAL_ITEMS,
  buildRows,
  resolveSelect,
  resolveBack,
  appliesOnSelect,
  screenTitle,
  screenDescription,
  verbosityBreadcrumb,
  type Screen,
  type SettingsSnapshot,
} from '../settings-panel-model.js';

const defaultSnapshot: SettingsSnapshot = {
  historyMode: 'session',
  interruptMode: 'steer',
};

/** Pull the id list off a screen's rows for terse reachability assertions. */
function rowIds(
  screen: Screen,
  snap: SettingsSnapshot = defaultSnapshot,
  uiMode?: 'tui' | 'lite'
) {
  return buildRows(screen, snap, uiMode).map((r) => r.id);
}

describe('settings-panel-model', () => {
  describe('top-level menu', () => {
    // verbosity is lite-only and must NOT appear in tui (or when uiMode is
    // omitted) — its handler errors with "lite mode only".
    it.each([[undefined], ['tui' as const]])(
      'exposes the five shared top items in order (uiMode=%s)',
      (uiMode) => {
        expect(rowIds({ type: 'top' }, defaultSnapshot, uiMode)).toEqual([
          'display',
          'theme',
          'terminal',
          'keybindings',
          'history',
        ]);
      }
    );

    it('splices the lite-only verbosity row in after display (lite)', () => {
      expect(rowIds({ type: 'top' }, defaultSnapshot, 'lite')).toEqual([
        'display',
        'verbosity',
        'theme',
        'terminal',
        'keybindings',
        'history',
      ]);
    });

    it('Terminal item advertises interrupt behaviour in its description', () => {
      const terminal = TOP_ITEMS.find((i) => i.id === 'terminal');
      // Regression guard: the description must mention interrupt behaviour so
      // the menu does not look like a newlines-only entry (which is how the
      // option got "lost" before).
      expect(terminal?.description.toLowerCase()).toContain('interrupt');
    });
  });

  describe('Terminal sub-screen reachability', () => {
    it('terminal sub-screen offers both newlines and interrupt behaviour', () => {
      expect(rowIds({ type: 'terminal' })).toEqual(['newlines', 'interrupt']);
      expect(TERMINAL_ITEMS.map((i) => i.id)).toEqual([
        'newlines',
        'interrupt',
      ]);
    });
  });

  describe('interrupt behaviour sub-screen', () => {
    it('offers steer and queue', () => {
      expect(rowIds({ type: 'terminal:interrupt' })).toEqual([
        'steer',
        'queue',
      ]);
    });
  });

  describe('history sub-screen', () => {
    it('offers session and global', () => {
      expect(rowIds({ type: 'history' })).toEqual(['session', 'global']);
    });
  });

  describe('resolveSelect routing', () => {
    // Every menu leaf maps to the correct screen-navigation or named action.
    // Regression guard: Terminal must NAVIGATE (not run newlines setup), and
    // Interrupt behaviour must be reachable as its own sub-screen.
    it.each([
      [{ type: 'top' }, 'display', { type: 'open-panel', panel: 'display' }],
      [{ type: 'top' }, 'theme', { type: 'open-panel', panel: 'theme' }],
      [
        { type: 'top' },
        'keybindings',
        { type: 'open-panel', panel: 'keybindings' },
      ],
      [{ type: 'top' }, 'verbosity', { type: 'open-verbosity' }],
      [{ type: 'terminal' }, 'newlines', { type: 'run-terminal-setup' }],
      [
        { type: 'terminal:interrupt' },
        'steer',
        { type: 'apply-interrupt', mode: 'steer' },
      ],
      [
        { type: 'terminal:interrupt' },
        'queue',
        { type: 'apply-interrupt', mode: 'queue' },
      ],
      [
        { type: 'history' },
        'session',
        { type: 'apply-history', mode: 'session' },
      ],
      [
        { type: 'history' },
        'global',
        { type: 'apply-history', mode: 'global' },
      ],
    ] as const)('%o + %s → action', (screen, id, action) => {
      expect(resolveSelect(screen, id)).toEqual({ kind: 'action', action });
    });

    it.each([
      [{ type: 'top' }, 'terminal', { type: 'terminal' }],
      [{ type: 'top' }, 'history', { type: 'history' }],
      [{ type: 'terminal' }, 'interrupt', { type: 'terminal:interrupt' }],
    ] as const)('%o + %s → navigate', (screen, id, target) => {
      expect(resolveSelect(screen, id)).toEqual({
        kind: 'navigate',
        screen: target,
      });
    });

    it.each([
      [{ type: 'top' } as const],
      [{ type: 'terminal' } as const],
      [{ type: 'terminal:interrupt' } as const],
      [{ type: 'history' } as const],
    ])('returns null for an unknown row id on %o', (screen) => {
      expect(resolveSelect(screen, 'bogus')).toBeNull();
    });
  });

  describe('active-marker rendering', () => {
    // The ● dot tags the persisted choice on each apply-on-select sub-screen.
    it.each([
      [
        { type: 'terminal:interrupt' } as const,
        'interruptMode' as const,
        'steer',
        ['Steer ●', 'Queue'],
      ],
      [
        { type: 'terminal:interrupt' } as const,
        'interruptMode' as const,
        'queue',
        ['Steer', 'Queue ●'],
      ],
      [
        { type: 'history' } as const,
        'historyMode' as const,
        'session',
        ['Session ●', 'Global'],
      ],
      [
        { type: 'history' } as const,
        'historyMode' as const,
        'global',
        ['Session', 'Global ●'],
      ],
    ])('%o marks active when %s=%s', (screen, key, value, labels) => {
      const rows = buildRows(screen, { ...defaultSnapshot, [key]: value });
      expect(rows.map((r) => r.values.label)).toEqual(labels);
    });
  });

  describe('back-navigation', () => {
    it('top closes the overlay', () => {
      expect(resolveBack({ type: 'top' })).toBe('close');
    });

    it('terminal and history return to top', () => {
      expect(resolveBack({ type: 'terminal' })).toEqual({ type: 'top' });
      expect(resolveBack({ type: 'history' })).toEqual({ type: 'top' });
    });

    it('interrupt sub-screen returns to terminal (one level up, not top)', () => {
      expect(resolveBack({ type: 'terminal:interrupt' })).toEqual({
        type: 'terminal',
      });
    });
  });

  describe('footer hint (appliesOnSelect)', () => {
    it('is true only for screens that apply-and-close', () => {
      expect(appliesOnSelect({ type: 'history' })).toBe(true);
      expect(appliesOnSelect({ type: 'terminal:interrupt' })).toBe(true);
      // Navigation-only screens keep the standard select hint.
      expect(appliesOnSelect({ type: 'top' })).toBe(false);
      expect(appliesOnSelect({ type: 'terminal' })).toBe(false);
    });
  });

  describe('titles and descriptions', () => {
    it('has a breadcrumb title per screen', () => {
      expect(screenTitle({ type: 'top' })).toBe('/settings');
      expect(screenTitle({ type: 'terminal' })).toBe('/settings – terminal');
      expect(screenTitle({ type: 'terminal:interrupt' })).toBe(
        '/settings – interrupt behaviour'
      );
      expect(screenTitle({ type: 'history' })).toBe('/settings – history');
    });

    it('top has no subtitle; sub-screens do', () => {
      expect(screenDescription({ type: 'top' })).toBeUndefined();
      expect(screenDescription({ type: 'terminal' })).toBeTruthy();
      expect(screenDescription({ type: 'terminal:interrupt' })).toBeTruthy();
      expect(screenDescription({ type: 'history' })).toBeTruthy();
    });
  });

  describe('verbosityBreadcrumb', () => {
    // Roots (top / unknown / undefined), one sub-screen per row, and every
    // truncation flavor (submenu, args/output fixtures, numeric editor) all
    // collapsing to the single truncation breadcrumb.
    it.each([
      ['top', '/settings – verbosity'],
      [undefined, '/settings – verbosity'],
      ['bogus', '/settings – verbosity'],
      ['density', '/settings – verbosity – density'],
      ['tool', '/settings – verbosity – tool calls'],
      ['subagent', '/settings – verbosity – subagent'],
      ['output', '/settings – verbosity – output'],
      ['truncation', '/settings – verbosity – truncation'],
      ['truncation:args', '/settings – verbosity – truncation'],
      ['truncation:output', '/settings – verbosity – truncation'],
      ['truncation:argsLines:edit', '/settings – verbosity – truncation'],
      ['truncation:outputChars:edit', '/settings – verbosity – truncation'],
    ] as const)('maps %s → %s', (previewKey, expected) => {
      expect(verbosityBreadcrumb(previewKey)).toBe(expected);
    });
  });
});
