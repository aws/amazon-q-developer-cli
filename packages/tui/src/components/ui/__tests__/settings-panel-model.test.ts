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
function rowIds(screen: Screen, snap: SettingsSnapshot = defaultSnapshot) {
  return buildRows(screen, snap).map((r) => r.id);
}

describe('settings-panel-model', () => {
  describe('top-level menu', () => {
    // Inside the Lite rollout (default true) the verbosity row is spliced in
    // after Display; lite always qualifies, TUI only in-cohort.
    it('splices the verbosity row in after display when rollout-enabled', () => {
      expect(rowIds({ type: 'top' })).toEqual([
        'display',
        'verbosity',
        'theme',
        'terminal',
        'keybindings',
        'history',
      ]);
    });

    // Off the rollout the port doesn't exist on the TUI: no verbosity row.
    it('drops the verbosity row off the rollout cohort', () => {
      const ids = buildRows({ type: 'top' }, defaultSnapshot, false).map(
        (r) => r.id
      );
      expect(ids).not.toContain('verbosity');
      expect(ids).toEqual([
        'display',
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

  describe('Display preview rollout gate', () => {
    // Parallels selectDisplayItems: the rollout-gated "Default UI at startup"
    // toggle is hidden off the cohort, so its preview clause drops too.
    const displayDesc = (rolloutEnabled: boolean) =>
      buildRows({ type: 'top' }, defaultSnapshot, rolloutEnabled).find(
        (r) => r.id === 'display'
      )?.values.description;

    it('drops "Default UI at startup" off the rollout, keeps it on', () => {
      expect(displayDesc(false)).not.toContain('Default UI');
      expect(displayDesc(true)).toContain('Default UI at startup');
    });
  });

  describe('sub-screen rows', () => {
    // Every sub-screen exposes its leaf rows in order (reachability guard);
    // terminal=[newlines,interrupt] pins the interrupt-dropped-out regression.
    it.each<[Screen, string[]]>([
      [{ type: 'terminal' }, ['newlines', 'interrupt']],
      [{ type: 'terminal:interrupt' }, ['steer', 'queue']],
      [{ type: 'history' }, ['session', 'global']],
    ])('%o rows', (screen, expected) => {
      expect(rowIds(screen)).toEqual(expected);
    });
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
