import { EditBufferChangedNotification } from "@aws/amazon-q-developer-cli-proto/fig";
import { NotificationResponse } from "./notifications.js";
export declare function subscribe(handler: (notification: EditBufferChangedNotification) => NotificationResponse | undefined): Promise<import("./notifications.js").Subscription> | undefined;
