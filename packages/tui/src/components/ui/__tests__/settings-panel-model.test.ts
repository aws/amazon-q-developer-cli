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
  type Screen,
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
    it('exposes the five top items in order', () => {
      expect(rowIds({ type: 'top' })).toEqual([
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

  describe('Terminal sub-screen reachability', () => {
    it('selecting Terminal navigates into the terminal sub-screen (not straight to newlines setup)', () => {
      const result = resolveSelect({ type: 'top' }, 'terminal');
      expect(result).toEqual({
        kind: 'navigate',
        screen: { type: 'terminal' },
      });
    });

    it('terminal sub-screen offers both newlines and interrupt behaviour', () => {
      expect(rowIds({ type: 'terminal' })).toEqual(['newlines', 'interrupt']);
      expect(TERMINAL_ITEMS.map((i) => i.id)).toEqual([
        'newlines',
        'interrupt',
      ]);
    });

    it('selecting Newlines runs the terminal setup flow', () => {
      const result = resolveSelect({ type: 'terminal' }, 'newlines');
      expect(result).toEqual({
        kind: 'action',
        action: { type: 'run-terminal-setup' },
      });
    });

    it('selecting Interrupt behaviour navigates into the interrupt sub-screen', () => {
      const result = resolveSelect({ type: 'terminal' }, 'interrupt');
      expect(result).toEqual({
        kind: 'navigate',
        screen: { type: 'terminal:interrupt' },
      });
    });
  });

  describe('interrupt behaviour sub-screen', () => {
    it('offers steer and queue', () => {
      expect(rowIds({ type: 'terminal:interrupt' })).toEqual([
        'steer',
        'queue',
      ]);
    });

    it('selecting steer applies the steer interrupt mode', () => {
      const result = resolveSelect({ type: 'terminal:interrupt' }, 'steer');
      expect(result).toEqual({
        kind: 'action',
        action: { type: 'apply-interrupt', mode: 'steer' },
      });
    });

    it('selecting queue applies the queue interrupt mode', () => {
      const result = resolveSelect({ type: 'terminal:interrupt' }, 'queue');
      expect(result).toEqual({
        kind: 'action',
        action: { type: 'apply-interrupt', mode: 'queue' },
      });
    });

    it('marks the active mode with a dot suffix (steer)', () => {
      const rows = buildRows(
        { type: 'terminal:interrupt' },
        { historyMode: 'session', interruptMode: 'steer' }
      );
      expect(rows[0]!.values.label).toBe('Steer ●');
      expect(rows[1]!.values.label).toBe('Queue');
    });

    it('marks the active mode with a dot suffix (queue)', () => {
      const rows = buildRows(
        { type: 'terminal:interrupt' },
        { historyMode: 'session', interruptMode: 'queue' }
      );
      expect(rows[0]!.values.label).toBe('Steer');
      expect(rows[1]!.values.label).toBe('Queue ●');
    });
  });

  describe('history sub-screen', () => {
    it('selecting Terminal-adjacent History navigates from top', () => {
      expect(resolveSelect({ type: 'top' }, 'history')).toEqual({
        kind: 'navigate',
        screen: { type: 'history' },
      });
    });

    it('applies session / global on select', () => {
      expect(resolveSelect({ type: 'history' }, 'session')).toEqual({
        kind: 'action',
        action: { type: 'apply-history', mode: 'session' },
      });
      expect(resolveSelect({ type: 'history' }, 'global')).toEqual({
        kind: 'action',
        action: { type: 'apply-history', mode: 'global' },
      });
    });

    it('marks the active history mode', () => {
      const rows = buildRows(
        { type: 'history' },
        { historyMode: 'global', interruptMode: 'steer' }
      );
      expect(rows[0]!.values.label).toBe('Session');
      expect(rows[1]!.values.label).toBe('Global ●');
    });
  });

  describe('top-level panel routing', () => {
    it('routes display/theme/keybindings to their own panels', () => {
      expect(resolveSelect({ type: 'top' }, 'display')).toEqual({
        kind: 'action',
        action: { type: 'open-panel', panel: 'display' },
      });
      expect(resolveSelect({ type: 'top' }, 'theme')).toEqual({
        kind: 'action',
        action: { type: 'open-panel', panel: 'theme' },
      });
      expect(resolveSelect({ type: 'top' }, 'keybindings')).toEqual({
        kind: 'action',
        action: { type: 'open-panel', panel: 'keybindings' },
      });
    });

    it('returns null for an unknown row id', () => {
      expect(resolveSelect({ type: 'top' }, 'bogus')).toBeNull();
      expect(resolveSelect({ type: 'terminal' }, 'bogus')).toBeNull();
      expect(resolveSelect({ type: 'terminal:interrupt' }, 'bogus')).toBeNull();
      expect(resolveSelect({ type: 'history' }, 'bogus')).toBeNull();
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

  describe('KAS gating', () => {
    it('omits the interrupt row from the terminal screen on KAS', () => {
      const ids = buildRows({ type: 'terminal' }, defaultSnapshot, 'kas').map(
        (r) => r.id
      );
      expect(ids).toContain('newlines');
      expect(ids).not.toContain('interrupt');
    });

    it('keeps the interrupt row on v2', () => {
      const ids = buildRows({ type: 'terminal' }, defaultSnapshot, 'v2').map(
        (r) => r.id
      );
      expect(ids).toContain('interrupt');
    });
  });
});
