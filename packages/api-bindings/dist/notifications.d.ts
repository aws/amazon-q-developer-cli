import { type Notification, type NotificationRequest, NotificationType } from "@aws/amazon-q-developer-cli-proto/fig";
export type NotificationResponse = {
    unsubscribe: boolean;
};
export type NotificationHandler = (notification: Notification) => NotificationResponse | undefined;
export interface Subscription {
    unsubscribe(): void;
}
export declare function _unsubscribe(type: NotificationType, handler?: NotificationHandler): void;
export declare function _subscribe(request: Omit<NotificationRequest, "$typeName">, handler: NotificationHandler): Promise<Subscription> | undefined;
