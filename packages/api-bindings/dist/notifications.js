import { NotificationRequestSchema, NotificationType, } from "@aws/amazon-q-developer-cli-proto/fig";
import { sendMessage } from "./core.js";
import { create } from "@bufbuild/protobuf";
const handlers = {};
export function _unsubscribe(type, handler) {
    if (handler && handlers[type] !== undefined) {
        handlers[type] = (handlers[type] ?? []).filter((x) => x !== handler);
    }
}
export function _subscribe(request, handler) {
    return new Promise((resolve, reject) => {
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
                let handlersToRemove;
                sendMessage({
                    case: "notificationRequest",
                    value: create(NotificationRequestSchema, request),
                }, (response) => {
                    switch (response?.case) {
                        case "notification":
                            if (!handlers[type]) {
                                return false;
                            }
                            // call handlers and remove any that have unsubscribed (by returning false)
                            handlersToRemove = handlers[type]?.filter((existingHandler) => {
                                const res = existingHandler(response.value);
                                return Boolean(res?.unsubscribe);
                            });
                            handlers[type] = handlers[type]?.filter((existingHandler) => !handlersToRemove?.includes(existingHandler));
                            return true;
                        case "success":
                            addHandler();
                            return true;
                        case "error":
                            reject(new Error(response.value));
                            break;
                        default:
                            reject(new Error("Not a notification"));
                            break;
                    }
                    return false;
                });
            }
            else {
                addHandler();
            }
        }
        else {
            reject(new Error("NotificationRequest type must be defined."));
        }
    });
}
const unsubscribeFromAll = () => {
    sendMessage({
        case: "notificationRequest",
        value: create(NotificationRequestSchema, {
            subscribe: false,
            type: NotificationType.ALL,
        }),
    });
};
if (!window?.fig?.quiet) {
    console.log("[q] unsubscribing any existing notifications...");
}
unsubscribeFromAll();
//# sourceMappingURL=notifications.js.map