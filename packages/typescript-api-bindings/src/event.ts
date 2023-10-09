import { NotificationType } from '@fig/fig-api-proto/dist/fig.pb';
import { _subscribe, NotificationResponse } from './notifications';

export function subscribe<T>(
  eventName: string,
  handler: (payload: T) => NotificationResponse | undefined
) {
  return _subscribe(
    { type: NotificationType.NOTIFY_ON_EVENT },
    notification => {
      switch (notification?.type?.$case) {
        case 'eventNotification':
          // eslint-disable-next-line no-case-declarations
          const { eventName: name, payload } = notification.type.eventNotification;
          if (name === eventName) {
            try {
              return handler(payload ? JSON.parse(payload) : null);
            } catch (err) {
              // ignore on json parse failure (invalid event).
            }
          }
          break;
        default:
          break;
      }

      return { unsubscribe: false };
    }
  );
}