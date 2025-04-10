import {
  LocalStateChangedNotification,
  NotificationType,
} from "@aws/amazon-q-developer-cli-proto/fig";
import { _subscribe, NotificationResponse } from "./notifications.js";

import {
  sendGetLocalStateRequest,
  sendUpdateLocalStateRequest,
} from "./requests.js";

export const didChange = {
  subscribe(
    handler: (
      notification: LocalStateChangedNotification,
    ) => NotificationResponse | undefined,
  ) {
    return _subscribe(
      { type: NotificationType.NOTIFY_ON_LOCAL_STATE_CHANGED },
      (notification) => {
        switch (notification?.type?.case) {
          case "localStateChangedNotification":
            return handler(notification.type.value);
          default:
            break;
        }

        return { unsubscribe: false };
      },
    );
  },
};

export async function get(key: string) {
  const response = await sendGetLocalStateRequest({ key });
  return response.jsonBlob ? JSON.parse(response.jsonBlob) : null;
}

export async function set(key: string, value: unknown): Promise<void> {
  return sendUpdateLocalStateRequest({
    key,
    value: JSON.stringify(value),
  });
}

export async function remove(key: string) {
  return sendUpdateLocalStateRequest({
    key,
  });
}

export async function current() {
  const all = await sendGetLocalStateRequest({});
  return JSON.parse(all.jsonBlob ?? "{}");
}
