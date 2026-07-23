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
    // Rows shared by both cohorts (all except the cohort-specific two).
    const shared = on.filter((k) => k !== Settings.CHAT_UI_MODE);
    for (const k of shared) expect(off).toContain(k);
    expect(off.length).toBeGreaterThan(0);
  });

  it('swaps Default UI (in-cohort) for Show thinking (off-cohort)', () => {
    const off = selectDisplayItems(false).map((i) => i.key);
    const on = selectDisplayItems(true).map((i) => i.key);
    // In-cohort: Default UI present, thinking lives in /verbosity (not here).
    expect(on).toContain(Settings.CHAT_UI_MODE);
    expect(on).not.toContain(Settings.CHAT_SHOW_THINKING);
    // Off-cohort: /verbosity is gated away, so the mainline thinking row is
    // restored here; Default UI is dropped (resolveUiMode forces tui).
    expect(off).not.toContain(Settings.CHAT_UI_MODE);
    expect(off).toContain(Settings.CHAT_SHOW_THINKING);
    // Show thinking sits just before Terminal title (mainline order).
    expect(off.indexOf(Settings.CHAT_SHOW_THINKING)).toBe(
      off.indexOf(Settings.CHAT_TERMINAL_TITLE) - 1
    );
  });
});
