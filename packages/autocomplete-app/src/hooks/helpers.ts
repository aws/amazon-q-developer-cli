import { useState, useEffect, useMemo, useCallback } from "react";
import { ResizeObserver } from "@juggle/resize-observer";

import useResizeObserver from "use-resize-observer";

if ("ResizeObserver" in window === false) {
  // eslint-disable-next-line @typescript-eslint/ban-ts-comment
  // @ts-ignore
  window.ResizeObserver = ResizeObserver;
}

type Dimensions = {
  height?: number;
  width?: number;
};

// Like resizeObserver but re-calls onResize callback when onResize changes.
export const useDynamicResizeObserver: typeof useResizeObserver = (opts) => {
  const { onResize, ...otherOpts } = opts || {};
  const [{ height, width }, setDimensions] = useState<Dimensions>({
    height: 0,
    width: 0,
  });

  const { ref } = useResizeObserver({ ...otherOpts, onResize: setDimensions });

  useEffect(() => {
    if (onResize) {
      onResize({ height, width });
    }
  }, [height, width, onResize]);

  return useMemo(() => ({ height, width, ref }), [height, width, ref]);
};

export const useShake = (): [boolean, () => void] => {
  const [isShaking, setShaking] = useState(false);
  const shake = useCallback((ms = 200) => {
    setShaking(true);
    setTimeout(() => {
      setShaking(false);
    }, ms);
  }, []);
  return [isShaking, shake];
};

export type SystemTheme = "light" | "dark";

export const useSystemTheme = (): [SystemTheme] => {
  const getSystemTheme = () =>
    window.matchMedia("(prefers-color-scheme: dark)").matches
      ? "dark"
      : "light";
  const [systemTheme, setSystemTheme] = useState<SystemTheme>(getSystemTheme());
  useEffect(() => {
    const listener = (e: MediaQueryListEvent) =>
      setSystemTheme(e.matches ? "dark" : "light");
    const query = window.matchMedia("(prefers-color-scheme: dark)");
    if (query.addEventListener) {
      query.addEventListener("change", listener);
    } else {
      query.addListener(listener);
    }
    return () =>
      query.removeEventListener
        ? query.removeEventListener("change", listener)
        : query.removeListener(listener);
  }, []);
  return [systemTheme];
};
