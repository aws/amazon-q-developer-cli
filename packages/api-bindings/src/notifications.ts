import {
  Notification,
  ServerOriginatedMessage,
  NotificationRequest,
  NotificationType,
} from "@aws/amazon-q-developer-cli-proto/fig";

import { sendMessage } from "./core.js";

export type NotificationResponse = {
  unsubscribe: boolean;
};

export type NotificationHandler = (
  notification: Notification,
) => NotificationResponse | undefined;
export interface Subscription {
  unsubscribe(): void;
}

const handlers: Partial<Record<NotificationType, NotificationHandler[]>> = {};

// eslint-disable-next-line no-underscore-dangle
export function _unsubscribe(
  type: NotificationType,
  handler?: NotificationHandler,
) {
  if (handler && handlers[type] !== undefined) {
    handlers[type] = (handlers[type] ?? []).filter((x) => x !== handler);
  }
}

// eslint-disable-next-line no-underscore-dangle
export function _subscribe(
  request: NotificationRequest,
  handler: NotificationHandler,
): Promise<Subscription> | undefined {
  return new Promise<Subscription>((resolve, reject) => {
    const { type } = request;

    if (type) {
      const addHandler = () => {
        handlers[type] = [...(handlers[type] ?? []), handler];
        resolve({ unsubscribe: () => _unsubscribe(type, handler) });
      };

      // primary subscription already exists
      if (handlers[type] === undefined) {
        handlers[type] = [];

        request.subscribe = true;

        let handlersToRemove: NotificationHandler[] | undefined;
        sendMessage(
          { $case: "notificationRequest", notificationRequest: request },
          (response: ServerOriginatedMessage["submessage"]) => {
            switch (response?.$case) {
              case "notification":
                if (!handlers[type]) {
                  return false;
                }

                // call handlers and remove any that have unsubscribed (by returning false)
                handlersToRemove = handlers[type]?.filter((existingHandler) => {
                  const res = existingHandler(response.notification);
                  return Boolean(res?.unsubscribe);
                });

                handlers[type] = handlers[type]?.filter(
                  (existingHandler) =>
                    !handlersToRemove?.includes(existingHandler),
                );

                return true;
              case "success":
                addHandler();
                return true;
              case "error":
                reject(new Error(response.error));
                break;
              default:
                reject(new Error("Not a notification"));
                break;
            }

            return false;
          },
        );
      } else {
        addHandler();
      }
    } else {
      reject(new Error("NotificationRequest type must be defined."));
    }
  });
}

const unsubscribeFromAll = () => {
  sendMessage({
    $case: "notificationRequest",
    notificationRequest: {
      subscribe: false,
      type: NotificationType.ALL,
    },
  });
};

// eslint-disable-next-line @typescript-eslint/ban-ts-comment
if (!window?.fig?.quiet) {
  console.log("[q] unsubscribing any existing notifications...");
}
unsubscribeFromAll();
