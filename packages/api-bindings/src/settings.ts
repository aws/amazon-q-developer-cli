import {
  SettingsChangedNotification,
  NotificationType,
} from "@aws/amazon-q-developer-cli-proto/fig";
import { _subscribe, NotificationResponse } from "./notifications.js";

import {
  sendGetSettingsPropertyRequest,
  sendUpdateSettingsPropertyRequest,
} from "./requests.js";

export const didChange = {
  subscribe(
    handler: (
      notification: SettingsChangedNotification,
    ) => NotificationResponse | undefined,
  ) {
    return _subscribe(
      { type: NotificationType.NOTIFY_ON_SETTINGS_CHANGE },
      (notification) => {
        switch (notification?.type?.case) {
          case "settingsChangedNotification":
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
  return sendGetSettingsPropertyRequest({
    key,
  });
}

export async function set(key: string, value: unknown): Promise<void> {
  return sendUpdateSettingsPropertyRequest({
    key,
    value: JSON.stringify(value),
  });
}

export async function remove(key: string): Promise<void> {
  return sendUpdateSettingsPropertyRequest({
    key,
  });
}

export async function current() {
  const all = await sendGetSettingsPropertyRequest({});
  return JSON.parse(all.jsonBlob ?? "{}");
}
