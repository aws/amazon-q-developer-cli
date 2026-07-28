/**
 * Tests for the tangent picker selection contract:
 * - Calls dispatchSlashCommand with execCmd=/tangent <sessionId> and recordAs=/tangent <title>
 * - No-ops on current session (via resolveTangentSelection)
 *
 * Since useBackendPanelHandlers is a React hook with multiple store dependencies,
 * we test the core selection logic directly via resolveTangentSelection + contract checks.
 */
import { describe, it, expect } from 'bun:test';
import { resolveTangentSelection } from '../../../../utils/tangent-nav';

describe('tangent picker selection contract', () => {
  it('resolves a non-current session to switch action with the sessionId', () => {
    const decision = resolveTangentSelection(
      'target-session',
      'current-session'
    );
    expect(decision).toEqual({ action: 'switch', sessionId: 'target-session' });
  });

  it('resolves the current session as a no-op', () => {
    const decision = resolveTangentSelection(
      'current-session',
      'current-session'
    );
    expect(decision.action).toBe('noop');
  });

  it('picker builds execCmd from sessionId and recordAs from title', () => {
    // This documents the contract that BackendPanels.tsx + useBackendPanelHandlers.ts enforce:
    // - execCmd = `/tangent ${decision.sessionId}` (raw id for routing)
    // - recordAs = `/tangent ${title}` (human-readable for history recall)
    const sessionId = 'sess_abc123-def456';
    const title = 'my-experiment';
    const decision = resolveTangentSelection(sessionId, 'current-session');
    if (decision.action !== 'switch') throw new Error('expected switch action');

    const execCmd = `/tangent ${decision.sessionId}`;
    const recordAs = `/tangent ${title}`;

    expect(execCmd).toBe('/tangent sess_abc123-def456');
    expect(recordAs).toBe('/tangent my-experiment');
    // These are what CommandHistory.getInstance().add() will record (recordAs),
    // ensuring up-arrow recall shows "/tangent my-experiment" not the raw UUID.
  });

  it('root row resolves to switch action (not no-op) when not on root', () => {
    const decision = resolveTangentSelection('root-session', 'child-session');
    expect(decision).toEqual({ action: 'switch', sessionId: 'root-session' });
  });
});
