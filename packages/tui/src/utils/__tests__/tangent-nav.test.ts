import { describe, it, expect } from 'bun:test';
import { resolveTangentSelection } from '../tangent-nav';

describe('resolveTangentSelection', () => {
  it('switches to a different session by id', () => {
    expect(resolveTangentSelection('sess-b', 'sess-a')).toEqual({
      action: 'switch',
      sessionId: 'sess-b',
    });
  });

  it('switches to the root session by id (not "go back one level")', () => {
    // Selecting root while on a deep descendant must jump to root, not parent.
    expect(resolveTangentSelection('sess-root', 'sess-deep')).toEqual({
      action: 'switch',
      sessionId: 'sess-root',
    });
  });

  it('no-ops when selecting the session you are already on', () => {
    expect(resolveTangentSelection('sess-a', 'sess-a')).toEqual({
      action: 'noop',
    });
  });

  it('no-ops on an empty selection', () => {
    expect(resolveTangentSelection('', 'sess-a')).toEqual({ action: 'noop' });
  });

  it('switches when there is no current session', () => {
    expect(resolveTangentSelection('sess-b', undefined)).toEqual({
      action: 'switch',
      sessionId: 'sess-b',
    });
  });
});
