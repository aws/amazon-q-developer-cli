import { ProcessChangedNotification, ShellPromptReturnedNotification, TextUpdate, HistoryUpdatedNotification } from "@aws/amazon-q-developer-cli-proto/fig";
import { NotificationResponse } from "./notifications.js";
export declare const processDidChange: {
    subscribe(handler: (notification: ProcessChangedNotification) => NotificationResponse | undefined): Promise<import("./notifications.js").Subscription> | undefined;
};
export declare const promptDidReturn: {
    subscribe(handler: (notification: ShellPromptReturnedNotification) => NotificationResponse | undefined): Promise<import("./notifications.js").Subscription> | undefined;
};
export declare const historyUpdated: {
    subscribe(handler: (notification: HistoryUpdatedNotification) => NotificationResponse | undefined): Promise<import("./notifications.js").Subscription> | undefined;
};
export declare function insert(text: string, request?: Omit<TextUpdate, "insertion" | "$typeName">, terminalSessionId?: string): Promise<void>;
