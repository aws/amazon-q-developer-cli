import { describe, expect, it } from 'bun:test';
import { isOutgoingSessionPush } from '../kas';

/**
 * Pins the switch-back race guard (see isOutgoingSessionPush's doc for the
 * full mechanism): the outgoing session's own pushes mid-load must be
 * dropped, everything else must be untouched.
 */
describe('isOutgoingSessionPush (switch-back race guard)', () => {
  const OUTGOING = 'sess_local-0f31';
  const TARGET = 'cloud-b9e0296f';

  it('drops the outgoing session’s tagged push while a load is in flight', () => {
    expect(isOutgoingSessionPush(TARGET, OUTGOING)).toBe(true);
  });

  it('is inert for a push tagged with the load’s target id (the equality guard governs it)', () => {
    expect(isOutgoingSessionPush(TARGET, TARGET)).toBe(false);
  });

  it('drops nothing when no load is in flight', () => {
    expect(isOutgoingSessionPush(null, OUTGOING)).toBe(false);
    expect(isOutgoingSessionPush(null, TARGET)).toBe(false);
  });

  it('leaves untagged pushes to the existing guards (cloud drop / local accept)', () => {
    expect(isOutgoingSessionPush(TARGET, undefined)).toBe(false);
    expect(isOutgoingSessionPush(null, undefined)).toBe(false);
  });

  it('a same-id reload (load target IS the active session) drops nothing', () => {
    expect(isOutgoingSessionPush(OUTGOING, OUTGOING)).toBe(false);
  });
});
