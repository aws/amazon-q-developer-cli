import { describe, it, expect } from 'bun:test';
import { selectDisplayItems } from '../DisplaySettingsPanel';
import { Settings } from '../../../constants/settings';

describe('selectDisplayItems — Default UI rollout gate', () => {
  const hasDefaultUi = (rolloutEnabled: boolean) =>
    selectDisplayItems(rolloutEnabled).some(
      (item) => item.key === Settings.CHAT_UI_MODE
    );

  it('hides the Default UI (tui/lite) row outside the rollout cohort', () => {
    expect(hasDefaultUi(false)).toBe(false);
  });

  it('shows the Default UI row inside the rollout cohort', () => {
    expect(hasDefaultUi(true)).toBe(true);
  });

  it('keeps the other display rows regardless of rollout', () => {
    const off = selectDisplayItems(false).map((i) => i.key);
    const on = selectDisplayItems(true).map((i) => i.key);
    // Only the Default UI row differs between cohorts.
    expect(on).toContain(Settings.CHAT_UI_MODE);
    expect(off).not.toContain(Settings.CHAT_UI_MODE);
    expect(off).toEqual(on.filter((k) => k !== Settings.CHAT_UI_MODE));
    expect(off.length).toBeGreaterThan(0);
  });
});
