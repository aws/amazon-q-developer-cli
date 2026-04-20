import { useState, useCallback } from 'react';
import { useMouse } from '../renderer.js';

export function useScrollableBox({
  contentHeight,
  viewHeight,
  lineHeight = 1,
}: {
  contentHeight: number;
  viewHeight: number;
  lineHeight?: number;
}) {
  const maxScroll = Math.max(0, contentHeight - viewHeight);
  const [scrollTop, setScrollTop] = useState(0);

  const scroll = useCallback(
    (delta: number) => {
      setScrollTop((prev) => Math.max(0, Math.min(maxScroll, prev + delta)));
    },
    [maxScroll]
  );

  const scrollToBottom = useCallback(() => {
    setScrollTop(maxScroll);
  }, [maxScroll]);

  const onKey = useCallback(
    (
      input: string,
      key: {
        upArrow: boolean;
        downArrow: boolean;
        pageUp: boolean;
        pageDown: boolean;
      }
    ) => {
      if (key.upArrow || input === 'k') {
        scroll(-lineHeight);
        return true;
      }
      if (key.downArrow || input === 'j') {
        scroll(lineHeight);
        return true;
      }
      if (key.pageUp) {
        scroll(-viewHeight);
        return true;
      }
      if (key.pageDown) {
        scroll(viewHeight);
        return true;
      }
      return false;
    },
    [scroll, lineHeight, viewHeight]
  );

  useMouse(
    useCallback(
      (event: { type: string }) => {
        if (event.type === 'scrollup') scroll(-lineHeight * 3);
        else if (event.type === 'scrolldown') scroll(lineHeight * 3);
      },
      [scroll, lineHeight]
    )
  );

  // Scrollbar: thumb is 5–9% of viewHeight, minimum 1 row
  const thumbSize = Math.max(
    1,
    Math.round(viewHeight * (maxScroll > 0 ? 0.09 : 1))
  );
  const thumbTop =
    maxScroll > 0
      ? Math.round((scrollTop / maxScroll) * (viewHeight - thumbSize))
      : 0;

  return {
    scrollTop,
    scroll,
    scrollToBottom,
    onKey,
    maxScroll,
    thumbSize,
    thumbTop,
  };
}
