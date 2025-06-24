import {
  getSetting,
  SETTINGS,
} from "@aws/amazon-q-developer-cli-api-bindings-wrappers";

// Default settings values.
export const MAX_WIDTH = 320;
export const AUTOCOMPLETE_HEIGHT = 140;
export const POPOUT_WIDTH = 200;

export const getMaxWidth = (): number => {
  const maxWidthSetting = getSetting(SETTINGS.WIDTH);
  const historyIncrease = ["history_only", "show"].includes(
    getSetting(SETTINGS.HISTORY_MODE) as string,
  )
    ? 50
    : 0;
  return typeof maxWidthSetting !== "number"
    ? MAX_WIDTH + historyIncrease
    : maxWidthSetting;
};

export const getMaxHeight = (): number => {
  const maxHeightSetting = getSetting(SETTINGS.HEIGHT);
  return typeof maxHeightSetting !== "number"
    ? AUTOCOMPLETE_HEIGHT
    : maxHeightSetting;
};
