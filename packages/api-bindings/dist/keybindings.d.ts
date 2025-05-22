import { Action, KeybindingPressedNotification } from "@aws/amazon-q-developer-cli-proto/fig";
import { NotificationResponse } from "./notifications.js";
export declare function pressed(handler: (notification: KeybindingPressedNotification) => NotificationResponse | undefined): Promise<import("./notifications.js").Subscription> | undefined;
export declare function setInterceptKeystrokes(actions: Omit<Action, "$typeName">[], intercept: boolean, globalIntercept?: boolean, currentTerminalSessionId?: string): void;
