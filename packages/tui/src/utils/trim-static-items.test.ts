import { describe, expect, it } from 'bun:test';
import { trimStaticItems, MAX_STATIC_ITEMS } from './trim-static-items.js';

function makeItems(n: number) {
  return Array.from({ length: n }, (_, i) => ({
    type: 'turn' as const,
    id: `turn-${i}`,
    turn: {},
  }));
}

describe('trimStaticItems', () => {
  it('keeps all items when under the cap', () => {
    const items = makeItems(199);
    const ids = new Set(items.map((i) => i.id));
    const removed = trimStaticItems(items, ids);
    expect(removed).toBe(0);
    expect(items).toHaveLength(199);
  });

  it('keeps all items at exactly the cap', () => {
    const items = makeItems(MAX_STATIC_ITEMS);
    const ids = new Set(items.map((i) => i.id));
    const removed = trimStaticItems(items, ids);
    expect(removed).toBe(0);
    expect(items).toHaveLength(MAX_STATIC_ITEMS);
  });

  it('keeps all items within the 10% hysteresis band', () => {
    const count = Math.floor(MAX_STATIC_ITEMS * 1.1);
    const items = makeItems(count);
    const ids = new Set(items.map((i) => i.id));
    const removed = trimStaticItems(items, ids);
    expect(removed).toBe(0);
    expect(items).toHaveLength(count);
  });

  it('trims to cap when exceeding the hysteresis band', () => {
    const count = Math.floor(MAX_STATIC_ITEMS * 1.1) + 1;
    const items = makeItems(count);
    const ids = new Set(items.map((i) => i.id));
    const removed = trimStaticItems(items, ids);
    expect(removed).toBe(count - MAX_STATIC_ITEMS);
    expect(items).toHaveLength(MAX_STATIC_ITEMS);
  });

  it('removes oldest items and keeps newest', () => {
    const items = makeItems(250);
    const ids = new Set(items.map((i) => i.id));
    trimStaticItems(items, ids);
    expect(items[0]!.id).toBe('turn-50');
    expect(items[items.length - 1]!.id).toBe('turn-249');
  });

  it('does not delete from emittedIds — prevents re-emission', () => {
    const items = makeItems(250);
    const ids = new Set(items.map((i) => i.id));
    trimStaticItems(items, ids);
    // All 250 IDs still in the set — trimmed items must not be re-appended
    expect(ids.size).toBe(250);
    expect(ids.has('turn-0')).toBe(true);
    expect(ids.has('turn-49')).toBe(true);
    expect(ids.has('turn-50')).toBe(true);
    expect(ids.has('turn-249')).toBe(true);
  });
});
