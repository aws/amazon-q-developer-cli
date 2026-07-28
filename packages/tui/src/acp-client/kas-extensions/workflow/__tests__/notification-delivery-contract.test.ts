import { describe, expect, it } from 'bun:test';
import { InterruptMode } from '../../../../constants/interrupt-mode.js';
import {
  WORKFLOW_NOTIFICATION_DELIVERY_CONTRACT,
  WORKFLOW_NOTIFICATION_DELIVERY_METHOD,
} from '../notification-delivery-contract.js';

describe('workflow notification delivery contract', () => {
  it('encodes the session-scoped policy request', () => {
    expect(
      WORKFLOW_NOTIFICATION_DELIVERY_CONTRACT.encode({
        sessionId: 'parent-session',
        delivery: InterruptMode.QUEUE,
      })
    ).toEqual({
      sessionId: 'parent-session',
      delivery: 'queue',
    });
    expect(WORKFLOW_NOTIFICATION_DELIVERY_CONTRACT.method).toBe(
      WORKFLOW_NOTIFICATION_DELIVERY_METHOD
    );
  });

  it('decodes only supported delivery modes', () => {
    expect(
      WORKFLOW_NOTIFICATION_DELIVERY_CONTRACT.decode({ delivery: 'steer' })
    ).toEqual({ delivery: InterruptMode.STEER });
    expect(
      WORKFLOW_NOTIFICATION_DELIVERY_CONTRACT.decode({ delivery: 'queue' })
    ).toEqual({ delivery: InterruptMode.QUEUE });
    expect(
      WORKFLOW_NOTIFICATION_DELIVERY_CONTRACT.decode({ delivery: 'later' })
    ).toBeNull();
    expect(WORKFLOW_NOTIFICATION_DELIVERY_CONTRACT.decode(null)).toBeNull();
  });
});
