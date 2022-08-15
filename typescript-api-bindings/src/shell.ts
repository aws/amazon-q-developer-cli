import {
  NotificationType,
  ProcessChangedNotification,
  ShellPromptReturnedNotification,
  TextUpdate,
  HistoryUpdatedNotification
} from "./fig.pb";
import { sendInsertTextRequest } from "./requests";
import { _subscribe } from "./notifications";

export const processDidChange = {
  subscribe(
    handler: (notification: ProcessChangedNotification) => boolean | undefined
  ) {
    return _subscribe(
      { type: NotificationType.NOTIFY_ON_PROCESS_CHANGED },
      (notification) => {
        switch (notification?.type?.$case) {
          case "processChangeNotification":
            return handler(notification.type.processChangeNotification);
          default:
            break;
        }

        return false;
      }
    );
  }
};

export const promptDidReturn = {
  subscribe(
    handler: (
      notification: ShellPromptReturnedNotification
    ) => boolean | undefined
  ) {
    return _subscribe(
      { type: NotificationType.NOTIFY_ON_PROMPT },
      (notification) => {
        switch (notification?.type?.$case) {
          case "shellPromptReturnedNotification":
            return handler(notification.type.shellPromptReturnedNotification);
          default:
            break;
        }

        return false;
      }
    );
  }
};

export const historyUpdated = {
  subscribe(
    handler: (notification: HistoryUpdatedNotification) => boolean | undefined
  ) {
    return _subscribe(
      { type: NotificationType.NOTIFY_ON_HISTORY_UPDATED },
      (notification) => {
        switch (notification?.type?.$case) {
          case "historyUpdatedNotification":
            return handler(notification.type.historyUpdatedNotification);
          default:
            break;
        }

        return false;
      }
    );
  }
};

export async function insert(
  text: string,
  request?: Omit<TextUpdate, "insertion">,
  terminalSessionId?: string
) {
  if (request) {
    return sendInsertTextRequest({
      terminalSessionId,
      type: { $case: "update", update: { ...request, insertion: text } }
    });
  }
  return sendInsertTextRequest({
    terminalSessionId,
    type: { $case: "text", text }
  });
}
