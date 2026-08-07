import { describe, expect, it } from 'bun:test';
import {
  mapReviewKey,
  REVIEW_HINT_KEY_WIDTH,
  REVIEW_KEY_HINTS,
  reviewFooterHints,
  type ReviewIntent,
} from '../keymap.js';
import type { Key } from '../../../hooks/useKeypress.js';

const KEY: Key = {
  upArrow: false,
  downArrow: false,
  leftArrow: false,
  rightArrow: false,
  pageUp: false,
  pageDown: false,
  home: false,
  end: false,
  return: false,
  escape: false,
  tab: false,
  backspace: false,
  delete: false,
  ctrl: false,
  meta: false,
  shift: false,
  paste: false,
};

const press = (input: string, over: Partial<Key> = {}): ReviewIntent | null =>
  mapReviewKey(input, { ...KEY, ...over });

describe('mapReviewKey', () => {
  it('ignores every ctrl combination it does not own', () => {
    // The app's global handler claims ctrl+g, ctrl+c and others, and twinki
    // delivers the keystroke to both handlers. Resolving all of ctrl here is
    // what stops `g` from also jumping the cursor.
    expect(press('g', { ctrl: true })).toBeNull();
    expect(press('c', { ctrl: true })).toBeNull();
    expect(press('n', { ctrl: true })).toBeNull();
    expect(press('q', { ctrl: true })).toBeNull();
    expect(press('e', { ctrl: true })).toBeNull();
  });

  it('ignores meta combinations', () => {
    expect(press('n', { meta: true })).toBeNull();
    expect(press('g', { meta: true })).toBeNull();
  });

  it('pages with pgup/pgdn and ctrl+u/ctrl+d', () => {
    expect(press('', { pageUp: true })).toEqual({
      type: 'move-page',
      direction: -1,
    });
    expect(press('', { pageDown: true })).toEqual({
      type: 'move-page',
      direction: 1,
    });
    expect(press('u', { ctrl: true })).toEqual({
      type: 'move-page',
      direction: -1,
    });
    expect(press('d', { ctrl: true })).toEqual({
      type: 'move-page',
      direction: 1,
    });
  });

  it('moves a line with arrows and j/k', () => {
    expect(press('', { upArrow: true })).toEqual({
      type: 'move-line',
      delta: -1,
    });
    expect(press('j')).toEqual({ type: 'move-line', delta: 1 });
    expect(press('k')).toEqual({ type: 'move-line', delta: -1 });
  });

  it('jumps to an edge with home/end and g/G', () => {
    expect(press('', { home: true })).toEqual({
      type: 'move-edge',
      edge: 'start',
    });
    expect(press('g')).toEqual({ type: 'move-edge', edge: 'start' });
    expect(press('G')).toEqual({ type: 'move-edge', edge: 'end' });
  });

  it('separates section jumps from comment jumps', () => {
    expect(press('n')).toEqual({ type: 'jump-section', direction: 1 });
    expect(press('N')).toEqual({ type: 'jump-section', direction: -1 });
    expect(press(']')).toEqual({ type: 'jump-comment', direction: 1 });
    expect(press('[')).toEqual({ type: 'jump-comment', direction: -1 });
  });

  it('comments with enter or e, and deletes with del or backspace', () => {
    expect(press('', { return: true })).toEqual({ type: 'comment' });
    expect(press('e')).toEqual({ type: 'comment' });
    expect(press('', { delete: true })).toEqual({ type: 'comment-delete' });
    expect(press('', { backspace: true })).toEqual({ type: 'comment-delete' });
  });

  it('closes on esc or q, and opens help on ?', () => {
    expect(press('', { escape: true })).toEqual({ type: 'close' });
    expect(press('q')).toEqual({ type: 'close' });
    expect(press('?')).toEqual({ type: 'help' });
  });

  it('owns no other printable key', () => {
    for (const input of ['c', 'd', 'b', ' ', 'x', 'z', 'v', '1']) {
      expect(press(input)).toBeNull();
    }
  });
});

describe('REVIEW_KEY_HINTS', () => {
  it('describes a key the handler actually maps', () => {
    // Guards the help and footer against describing a binding that moved.
    const firstKey: Record<string, [string, Partial<Key>]> = {
      'up down / j k': ['j', {}],
      'pgup pgdn / ctrl+u ctrl+d': ['', { pageUp: true }],
      'n N': ['n', {}],
      '] [': [']', {}],
      'home end / g G': ['', { home: true }],
      'enter / e': ['', { return: true }],
      del: ['', { delete: true }],
      '?': ['?', {}],
      'esc / q': ['', { escape: true }],
    };
    for (const hint of REVIEW_KEY_HINTS) {
      const probe = firstKey[hint.keys];
      expect(probe, `no probe for "${hint.keys}"`).toBeDefined();
      expect(press(probe![0], probe![1]), hint.keys).not.toBeNull();
    }
  });

  it('keeps both footer shapes inside one row on an 80-column terminal', () => {
    // The `to` phrasing matches every other menu, which leaves little room:
    // both the browsing footer and the one shown on a comment have to fit.
    for (const onComment of [false, true]) {
      const rendered = reviewFooterHints(onComment)
        .map((hint) => `${hint.glyph ? '↑↓' : hint.keys} ${hint.action}`)
        .join(' · ');
      expect(rendered.length, rendered).toBeLessThanOrEqual(80);
    }
  });

  it('leads the footer with esc and closes it with ?, as other menus do', () => {
    const actions = reviewFooterHints(false).map((hint) => hint.action);
    expect(actions[0]).toBe('to go back');
    expect(actions[actions.length - 1]).toBe('to see keys');
    expect(actions.every((action) => action.startsWith('to '))).toBe(true);
  });

  it('trades the section jump for the comment actions on a comment', () => {
    const browsing = reviewFooterHints(false).map((h) => h.action);
    const onComment = reviewFooterHints(true).map((h) => h.action);
    expect(browsing).toContain('to jump');
    expect(browsing).toContain('to comment');
    expect(onComment).not.toContain('to jump');
    expect(onComment).toContain('to edit');
    expect(onComment).toContain('to delete');
  });

  it('leaves a gap between the help list keys and their labels', () => {
    const widest = Math.max(
      ...REVIEW_KEY_HINTS.map((hint) => hint.keys.length)
    );
    expect(REVIEW_HINT_KEY_WIDTH).toBeGreaterThan(widest);
  });
});
