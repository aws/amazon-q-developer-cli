import { afterEach, describe, expect, it, vi } from "vitest";
import { isBuiltInTheme, setTheme, builtInThemes, mapColor } from "../themes";

describe("isBuiltInTheme", () => {
  it("returns whether a theme is built into fig", () => {
    expect(isBuiltInTheme("default")).toBe(false);
    expect(isBuiltInTheme("light")).toBe(true);
    expect(isBuiltInTheme("dark")).toBe(true);
    expect(isBuiltInTheme("custom")).toBe(false);
    expect(isBuiltInTheme("red")).toBe(false);
    expect(isBuiltInTheme("nightowl")).toBe(false);
  });
});

describe("setTheme", () => {
  const spy = vi.spyOn(document.documentElement.style, "setProperty");

  afterEach(() => {
    spy.mockReset();
  });
  it("sets the css variables correctly for the light theme", () => {
    setTheme("dark", "light");
    expect(spy).toHaveBeenNthCalledWith(
      1,
      "--main-bg-color",
      mapColor(builtInThemes.light.theme?.backgroundColor ?? ""),
    );
    spy.mockReset();
  });

  it("sets the css variables correctly for the dark theme", () => {
    setTheme("light", "dark");
    expect(spy).toHaveBeenNthCalledWith(
      1,
      "--main-bg-color",
      mapColor(builtInThemes.dark.theme?.backgroundColor ?? ""),
    );
  });
});
