import {
  InterruptMode,
  type InterruptMode as InterruptModeValue,
} from '../../../constants/interrupt-mode.js';
import type { RpcContract } from '../runtime.js';

export const WORKFLOW_NOTIFICATION_DELIVERY_METHOD =
  '_kiro/session/setWorkflowNotificationDelivery';

export interface WorkflowNotificationDeliveryRequest {
  sessionId: string;
  delivery: InterruptModeValue;
}

function decodeDelivery(
  value: unknown
): { delivery: InterruptModeValue } | null {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) {
    return null;
  }
  const delivery = (value as Record<string, unknown>).delivery;
  return delivery === InterruptMode.STEER || delivery === InterruptMode.QUEUE
    ? { delivery }
    : null;
}

export const WORKFLOW_NOTIFICATION_DELIVERY_CONTRACT: RpcContract<
  WorkflowNotificationDeliveryRequest,
  { delivery: InterruptModeValue }
> = {
  method: WORKFLOW_NOTIFICATION_DELIVERY_METHOD,
  encode: ({ sessionId, delivery }) => ({ sessionId, delivery }),
  decode: decodeDelivery,
};
