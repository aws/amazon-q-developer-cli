import { describe, expect, it } from 'bun:test';
import {
  BACKEND_PANEL_STATE_KEYS,
  selectBackendPanelVisibility,
} from '../BackendPanels.js';

type BackendPanelState = Parameters<typeof selectBackendPanelVisibility>[0];

function closedPanelState(): BackendPanelState {
  return Object.fromEntries(
    BACKEND_PANEL_STATE_KEYS.map((key) => [key, false])
  ) as unknown as BackendPanelState;
}

describe('backend panel visibility', () => {
  it('reports closed when no backend panel is visible', () => {
    expect(selectBackendPanelVisibility(closedPanelState())).toEqual({
      any: false,
      inlineHeader: false,
      inlineInput: false,
      inlineCopyHint: false,
    });
  });

  it('recognizes every canonical panel state flag', () => {
    expect(new Set(BACKEND_PANEL_STATE_KEYS).size).toBe(
      BACKEND_PANEL_STATE_KEYS.length
    );
    for (const key of BACKEND_PANEL_STATE_KEYS) {
      expect(
        selectBackendPanelVisibility({
          ...closedPanelState(),
          [key]: true,
        }).any,
        key
      ).toBe(true);
    }
  });

  it('preserves InlineLayout panel-specific gating', () => {
    const visibilityFor = (key: keyof BackendPanelState) =>
      selectBackendPanelVisibility({
        ...closedPanelState(),
        [key]: true,
      });

    expect(visibilityFor('showHelpPanel')).toEqual({
      any: true,
      inlineHeader: true,
      inlineInput: true,
      inlineCopyHint: true,
    });
    expect(visibilityFor('showGoalPanel')).toEqual({
      any: true,
      inlineHeader: true,
      inlineInput: false,
      inlineCopyHint: false,
    });
    expect(visibilityFor('showStatsPanel')).toEqual({
      any: true,
      inlineHeader: true,
      inlineInput: true,
      inlineCopyHint: false,
    });
    expect(visibilityFor('showRepoPicker')).toEqual({
      any: true,
      inlineHeader: true,
      inlineInput: true,
      inlineCopyHint: true,
    });
  });
});
