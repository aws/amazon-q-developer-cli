import {
  Action,
  KeybindingPressedNotification,
  NotificationType,
} from "@aws/amazon-q-developer-cli-proto/fig";
import { sendUpdateApplicationPropertiesRequest } from "./requests.js";
import { _subscribe, NotificationResponse } from "./notifications.js";

export function pressed(
  handler: (
    notification: KeybindingPressedNotification,
  ) => NotificationResponse | undefined,
) {
  return _subscribe(
    { type: NotificationType.NOTIFY_ON_KEYBINDING_PRESSED },
    (notification) => {
      switch (notification?.type?.$case) {
        case "keybindingPressedNotification":
          return handler(notification.type.keybindingPressedNotification);
        default:
          break;
      }

      return { unsubscribe: false };
    },
  );
}

export function setInterceptKeystrokes(
  actions: Action[],
  intercept: boolean,
  globalIntercept?: boolean,
  currentTerminalSessionId?: string,
) {
  sendUpdateApplicationPropertiesRequest({
    interceptBoundKeystrokes: intercept,
    interceptGlobalKeystrokes: globalIntercept,
    actionList: { actions },
    currentTerminalSessionId,
  });
}
