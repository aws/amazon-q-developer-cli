import { EditBufferChangedNotification, NotificationType } from './fig.pb';
import { _subscribe } from './notifications';

export function subscribe(
  handler: (notification: EditBufferChangedNotification) => boolean | undefined
) {
  return _subscribe(
    { type: NotificationType.NOTIFICATION_TYPE_NOTIFY_ON_EDITBUFFFER_CHANGE },
    notification => {
      switch (notification?.type?.$case) {
        case 'editBufferNotification':
          return handler(notification.type.editBufferNotification);
        default:
          break;
      }

      return false;
    }
  );
}
