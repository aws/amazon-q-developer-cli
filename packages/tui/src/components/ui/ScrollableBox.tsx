import React, {
  useState,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
} from 'react';
import { Box, measureElement, Scrollbar, useMouse } from '../../renderer.js';
import { useKeypress } from '../../hooks/useKeypress.js';
import type { DOMElement } from 'ink';

export interface ScrollableBoxProps {
  height: number;
  width?: number;
  autoScroll?: boolean;
  children: React.ReactNode;
}

export const ScrollableBox: React.FC<ScrollableBoxProps> = ({
  height,
  width,
  autoScroll = true,
  children,
}) => {
  const [scrollTop, setScrollTop] = useState(0);
  const [contentHeight, setContentHeight] = useState(0);
  const contentRef = useRef<DOMElement>(null);
  const containerRef = useRef<DOMElement>(null);
  const [containerWidth, setContainerWidth] = useState(0);

  // Measure content after layout, throttled to avoid blocking the main thread
  // during rapid streaming updates. We use useLayoutEffect to schedule a
  // deferred measurement rather than measuring synchronously on every render.
  const measurePending = useRef(false);
  const measureTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useLayoutEffect(() => {
    // Skip if a measurement is already scheduled
    if (measurePending.current) return;
    measurePending.current = true;

    // Defer measurement to next frame so we batch rapid updates
    measureTimer.current = setTimeout(() => {
      measurePending.current = false;
      if (containerRef.current) {
        const { width: w } = measureElement(containerRef.current);
        setContainerWidth(w);
      }
      if (contentRef.current) {
        const { height: measured } = measureElement(contentRef.current);
        setContentHeight(measured);
      }
    }, 32);
  });

  // Cleanup timer on unmount
  useEffect(() => {
    return () => {
      if (measureTimer.current) clearTimeout(measureTimer.current);
    };
  }, []);

  const totalLines = contentHeight;
  const maxScroll = Math.max(0, totalLines - height);
  const maxScrollRef = useRef(maxScroll);
  maxScrollRef.current = maxScroll;

  const clampedScrollTop = Math.min(scrollTop, maxScroll);

  const userScrolledUp = useRef(false);

  const scroll = useCallback((delta: number) => {
    setScrollTop((prev) => {
      const next = Math.max(0, Math.min(maxScrollRef.current, prev + delta));
      userScrolledUp.current = next < maxScrollRef.current;
      return next;
    });
  }, []);

  // Auto-scroll when content grows, only if user is at the bottom
  const prevContentHeight = useRef(contentHeight);
  useEffect(() => {
    if (
      autoScroll &&
      contentHeight > prevContentHeight.current &&
      !userScrolledUp.current
    ) {
      setScrollTop(maxScrollRef.current);
    }
    prevContentHeight.current = contentHeight;
  }, [contentHeight, autoScroll]);

  // Reset scroll when height changes
  const prevHeight = useRef(height);
  useEffect(() => {
    if (height !== prevHeight.current) {
      prevHeight.current = height;
      setScrollTop((s) => Math.min(s, maxScrollRef.current));
    }
  }, [height]);

  useKeypress((input, key) => {
    if (input === 'k') scroll(-1);
    else if (input === 'j') scroll(1);
    else if (key.pageUp || (key.ctrl && input === 'u'))
      scroll(-Math.floor(height / 2));
    else if (key.pageDown || (key.ctrl && input === 'd'))
      scroll(Math.floor(height / 2));
  });

  const mouseResult = useMouse({
    onScrollUp: useCallback(() => scroll(-3), [scroll]),
    onScrollDown: useCallback(() => scroll(3), [scroll]),
  });
  const mouseRef = mouseResult?.ref;

  const showScrollbar = maxScroll > 0;

  return (
    <Box
      ref={(node: any) => {
        (containerRef as any).current = node;
        if (mouseRef) {
          if (typeof mouseRef === 'function') mouseRef(node);
          else (mouseRef as any).current = node;
        }
      }}
      flexDirection="row"
      height={height}
      width={width}
      overflow="hidden"
    >
      <Box
        width={
          showScrollbar && containerWidth > 1 ? containerWidth - 1 : undefined
        }
        flexGrow={showScrollbar ? undefined : 1}
        height={height}
        overflow="hidden"
        flexDirection="column"
        scrollTop={clampedScrollTop}
      >
        <Box ref={contentRef} flexDirection="column" flexShrink={0}>
          {children}
        </Box>
      </Box>
      {showScrollbar && containerWidth > 1 && (
        <Scrollbar
          scrollTop={clampedScrollTop}
          totalLines={totalLines}
          viewportHeight={height}
        />
      )}
    </Box>
  );
};
