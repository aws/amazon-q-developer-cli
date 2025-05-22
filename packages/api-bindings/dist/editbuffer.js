import { NotificationType, } from "@aws/amazon-q-developer-cli-proto/fig";
import { _subscribe } from "./notifications.js";
export function subscribe(handler) {
    return _subscribe({ type: NotificationType.NOTIFY_ON_EDITBUFFFER_CHANGE }, (notification) => {
        switch (notification?.type?.case) {
            case "editBufferNotification":
                return handler(notification.type.value);
            default:
                break;
        }
        return { unsubscribe: false };
    });
}
//# sourceMappingURL=editbuffer.js.map