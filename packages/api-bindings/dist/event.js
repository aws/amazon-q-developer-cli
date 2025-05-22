import { NotificationType } from "@aws/amazon-q-developer-cli-proto/fig";
import { _subscribe } from "./notifications.js";
export function subscribe(eventName, handler) {
    return _subscribe({ type: NotificationType.NOTIFY_ON_EVENT }, (notification) => {
        switch (notification?.type?.case) {
            case "eventNotification": {
                const { eventName: name, payload } = notification.type.value;
                if (name === eventName) {
                    try {
                        return handler(payload ? JSON.parse(payload) : null);
                    }
                    catch (_err) {
                        // ignore on json parse failure (invalid event).
                    }
                }
                break;
            }
            default:
                break;
        }
        return { unsubscribe: false };
    });
}
//# sourceMappingURL=event.js.map