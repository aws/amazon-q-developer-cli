import { NotificationType, } from "@aws/amazon-q-developer-cli-proto/fig";
import { _subscribe } from "./notifications.js";
import { sendGetLocalStateRequest, sendUpdateLocalStateRequest, } from "./requests.js";
export const didChange = {
    subscribe(handler) {
        return _subscribe({ type: NotificationType.NOTIFY_ON_LOCAL_STATE_CHANGED }, (notification) => {
            switch (notification?.type?.case) {
                case "localStateChangedNotification":
                    return handler(notification.type.value);
                default:
                    break;
            }
            return { unsubscribe: false };
        });
    },
};
export async function get(key) {
    const response = await sendGetLocalStateRequest({ key });
    return response.jsonBlob ? JSON.parse(response.jsonBlob) : null;
}
export async function set(key, value) {
    return sendUpdateLocalStateRequest({
        key,
        value: JSON.stringify(value),
    });
}
export async function remove(key) {
    return sendUpdateLocalStateRequest({
        key,
    });
}
export async function current() {
    const all = await sendGetLocalStateRequest({});
    return JSON.parse(all.jsonBlob ?? "{}");
}
//# sourceMappingURL=state.js.map