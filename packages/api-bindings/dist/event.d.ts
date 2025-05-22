import { NotificationResponse } from "./notifications.js";
export declare function subscribe<T>(eventName: string, handler: (payload: T) => NotificationResponse | undefined): Promise<import("./notifications.js").Subscription> | undefined;
