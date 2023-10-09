import { LocalStateChangedNotification, NotificationType } from '@fig/fig-api-proto/dist/fig.pb';
import { _subscribe, NotificationResponse } from './notifications';

import {
  sendGetLocalStateRequest,
  sendUpdateLocalStateRequest
} from './requests';

export const didChange = {
  subscribe(
    handler: (notification: LocalStateChangedNotification) => NotificationResponse | undefined
  ) {
    return _subscribe(
      { type: NotificationType.NOTIFY_ON_LOCAL_STATE_CHANGED },
      notification => {
        switch (notification?.type?.$case) {
          case 'localStateChangedNotification':
            return handler(notification.type.localStateChangedNotification);
          default:
            break;
        }

        return { unsubscribe: false };
      }
    );
  }
};

export async function get(key: string) {
  const response = await sendGetLocalStateRequest({ key });
  return response.jsonBlob
    ? JSON.parse(response.jsonBlob)
    : null;
}

// eslint-disable-next-line @typescript-eslint/no-explicit-any
export async function set(key: string, value: any): Promise<void> {
  return sendUpdateLocalStateRequest({
    key,
    value: JSON.stringify(value)
  });
}

export async function remove(key: string) {
  return sendUpdateLocalStateRequest({
    key
  });
}

export async function current() {
  const all = await sendGetLocalStateRequest({});
  return JSON.parse(all.jsonBlob ?? '{}');
}