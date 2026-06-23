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
  buildRows,
  resolveSelect,
  resolveBack,
  appliesOnSelect,
  screenTitle,
  screenDescription,
  verbosityBreadcrumb,
  type Screen,
  type ScreenType,
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

  // Per-screen metadata over the four screens: leaf row ids (reachability —
  // terminal=[newlines,interrupt] pins the interrupt-dropped-out regression),
  // ESC back target, apply-and-close footer hint, breadcrumb title, and
  // whether a subtitle exists (top has none, sub-screens do).
  describe('screen metadata', () => {
    it.each<{
      screen: Screen;
      rows: string[];
      back: Screen | 'close';
      applies: boolean;
      title: string;
      hasDescription: boolean;
    }>([
      {
        screen: { type: 'top' },
        rows: ['display', 'theme', 'terminal', 'keybindings', 'history'],
        back: 'close',
        applies: false,
        title: '/settings',
        hasDescription: false,
      },
      {
        screen: { type: 'terminal' },
        rows: ['newlines', 'interrupt'],
        back: { type: 'top' },
        applies: false,
        title: '/settings – terminal',
        hasDescription: true,
      },
      {
        screen: { type: 'terminal:interrupt' },
        rows: ['steer', 'queue'],
        back: { type: 'terminal' }, // one level up, not top
        applies: true,
        title: '/settings – interrupt behaviour',
        hasDescription: true,
      },
      {
        screen: { type: 'history' },
        rows: ['session', 'global'],
        back: { type: 'top' },
        applies: true,
        title: '/settings – history',
        hasDescription: true,
      },
    ])(
      '$screen.type',
      ({ screen, rows, back, applies, title, hasDescription }) => {
        expect(rowIds(screen)).toEqual(rows);
        expect(resolveBack(screen)).toEqual(back);
        expect(appliesOnSelect(screen)).toBe(applies);
        expect(screenTitle(screen)).toBe(title);
        expect(screenDescription(screen) !== undefined).toBe(hasDescription);
      }
    );
  });

  describe('resolveSelect routing', () => {
    // Every menu leaf maps to the correct screen-navigation or named action.
    // Regression guard: Terminal must NAVIGATE (not run newlines setup), and
    // Interrupt behaviour must be reachable as its own sub-screen.
    type T = ScreenType;
    it.each<[T, string, object]>([
      ['top', 'display', { type: 'open-panel', panel: 'display' }],
      ['top', 'theme', { type: 'open-panel', panel: 'theme' }],
      ['top', 'keybindings', { type: 'open-panel', panel: 'keybindings' }],
      ['top', 'verbosity', { type: 'open-verbosity' }],
      ['terminal', 'newlines', { type: 'run-terminal-setup' }],
      [
        'terminal:interrupt',
        'steer',
        { type: 'apply-interrupt', mode: 'steer' },
      ],
      [
        'terminal:interrupt',
        'queue',
        { type: 'apply-interrupt', mode: 'queue' },
      ],
      ['history', 'session', { type: 'apply-history', mode: 'session' }],
      ['history', 'global', { type: 'apply-history', mode: 'global' }],
    ])('%s + %s → action', (type, id, action) => {
      expect(resolveSelect({ type }, id)).toEqual({
        kind: 'action',
        action,
      } as ReturnType<typeof resolveSelect>);
    });

    it.each<[T, string, T]>([
      ['top', 'terminal', 'terminal'],
      ['top', 'history', 'history'],
      ['terminal', 'interrupt', 'terminal:interrupt'],
    ])('%s + %s → navigate', (type, id, target) => {
      expect(resolveSelect({ type }, id)).toEqual({
        kind: 'navigate',
        screen: { type: target },
      });
    });

    it.each<[T]>([['top'], ['terminal'], ['terminal:interrupt'], ['history']])(
      'returns null for an unknown row id on %s',
      (type) => {
        expect(resolveSelect({ type }, 'bogus')).toBeNull();
      }
    );
  });

  describe('active-marker rendering', () => {
    // The ● dot tags the persisted choice on each apply-on-select sub-screen.
    type T = ScreenType;
    type K = keyof SettingsSnapshot;
    it.each<[T, K, string, string[]]>([
      ['terminal:interrupt', 'interruptMode', 'steer', ['Steer ●', 'Queue']],
      ['terminal:interrupt', 'interruptMode', 'queue', ['Steer', 'Queue ●']],
      ['history', 'historyMode', 'session', ['Session ●', 'Global']],
      ['history', 'historyMode', 'global', ['Session', 'Global ●']],
    ])('%s marks active when %s=%s', (type, key, value, labels) => {
      const rows = buildRows({ type }, { ...defaultSnapshot, [key]: value });
      expect(rows.map((r) => r.values.label)).toEqual(labels);
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
