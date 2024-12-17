import logger from "loglevel";

export const captureError = (err: Error, log = true) => {
  if (log) {
    logger.error(err);
  }
  // if (!telemetryDisabled()) {
  //   if (!didInitSentry) {
  //     initSentry();
  //   }
  //   Sentry.captureException(err);
  // }
};
