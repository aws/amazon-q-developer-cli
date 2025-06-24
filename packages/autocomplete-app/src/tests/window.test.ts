import {
  SETTINGS,
  updateSettings,
} from "@aws/amazon-q-developer-cli-api-bindings-wrappers";
import { beforeEach, describe, expect, it } from "vitest";
import {
  getMaxWidth,
  getMaxHeight,
  MAX_WIDTH,
  AUTOCOMPLETE_HEIGHT,
} from "../window";

describe("getMaxWidth", () => {
  beforeEach(() => {
    updateSettings({});
  });

  it("should return the default width", () => {
    expect(getMaxWidth()).toEqual(MAX_WIDTH);
  });

  it("should return the width from settings", () => {
    const width = Math.floor(Math.random() * 10 + 1);
    updateSettings({ [SETTINGS.WIDTH]: width });
    expect(getMaxWidth()).toEqual(width);
  });
});

describe("getMaxHeight", () => {
  beforeEach(() => {
    updateSettings({});
  });

  it("should return the default height", () => {
    expect(getMaxHeight()).toEqual(AUTOCOMPLETE_HEIGHT);
  });

  it("should return the height from settings", () => {
    const height = Math.floor(Math.random() * 10 + 1);
    updateSettings({ [SETTINGS.HEIGHT]: height });
    expect(getMaxHeight()).toEqual(height);
  });
});
