import { NotificationType, } from "@aws/amazon-q-developer-cli-proto/fig";
import { _subscribe } from "./notifications.js";
import { sendGetSettingsPropertyRequest, sendUpdateSettingsPropertyRequest, } from "./requests.js";
export const didChange = {
    subscribe(handler) {
        return _subscribe({ type: NotificationType.NOTIFY_ON_SETTINGS_CHANGE }, (notification) => {
            switch (notification?.type?.case) {
                case "settingsChangedNotification":
                    return handler(notification.type.value);
                default:
                    break;
            }
            return { unsubscribe: false };
        });
    },
};
export async function get(key) {
    return sendGetSettingsPropertyRequest({
        key,
    });
}
export async function set(key, value) {
    return sendUpdateSettingsPropertyRequest({
        key,
        value: JSON.stringify(value),
    });
}
export async function remove(key) {
    return sendUpdateSettingsPropertyRequest({
        key,
    });
}
export async function current() {
    const all = await sendGetSettingsPropertyRequest({});
    return JSON.parse(all.jsonBlob ?? "{}");
}
//# sourceMappingURL=settings.js.map