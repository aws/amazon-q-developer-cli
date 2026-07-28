import { describe, it, expect } from 'bun:test';
import { createAppStore, buildCommandContext } from '../app-store';
import { Kiro } from '../../kiro';

// Pins the tangent-chip reset in clearUIState against the real store action.
// Other suites stub clearUIState, so without this a deletion of the
// `tangentName: null` line in buildCommandContext would leave the suite green.
describe('app-store clearUIState — tangent chip', () => {
  it('resets tangentName to null', () => {
    const store = createAppStore({ kiro: new Kiro() });

    store.getState().setTangentName('experiment');
    expect(store.getState().tangentName).toBe('experiment');

    // clearUIState lives on the CommandContext built from the store's
    // set/get, not as a top-level getState() action.
    const ctx = buildCommandContext(
      store.getState() as any,
      store.setState,
      store.getState
    );
    ctx.clearUIState();

    expect(store.getState().tangentName).toBeNull();
  });
});
