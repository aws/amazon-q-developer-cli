import { describe, it, expect } from 'bun:test';
import { resolvePreserveScrollback } from '../renderer-options';

describe('resolvePreserveScrollback', () => {
  it('enables whenever the setting is on, on every surface', () => {
    expect(resolvePreserveScrollback({ settingEnabled: true })).toBe(true);
  });

  it('stays off by default', () => {
    expect(resolvePreserveScrollback({ settingEnabled: false })).toBe(false);
  });
});
