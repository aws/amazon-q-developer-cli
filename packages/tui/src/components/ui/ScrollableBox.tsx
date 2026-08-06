import React, {
  useState,
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
} from 'react';
import { Box, measureElement, Scrollbar, useMouse } from '../../renderer.js';
import { useKeypress } from '../../hooks/useKeypress.js';
export interface ScrollableBoxProps {
  height: number;
  width?: number;
  autoScroll?: boolean;
  /**
   * When false, the j/k/page scroll key handler is suspended so this box
   * doesn't claim those keys while another surface (e.g. an open composer or
   * a different pane) owns input. Mouse-wheel scrolling stays active. Defaults
   * to true to preserve existing call sites.
   */
  isActive?: boolean;
  /**
   * #13: change this value to force the view to jump to the end of the content
   * (e.g. when a workflow step pauses to ask a question and the actionable tail
   * would otherwise sit clipped below the fold). Unlike {@link autoScroll} —
   * which only reacts to content *growth* while the user is at the bottom — a
   * changed key scrolls to the bottom unconditionally, then clears the
   * "user scrolled up" latch so subsequent growth keeps following.
   */
  scrollToEndKey?: string | number;
  children: React.ReactNode;
}

export const ScrollableBox: React.FC<ScrollableBoxProps> = ({
  height,
  width,
  autoScroll = true,
  isActive = true,
  scrollToEndKey,
  children,
}) => {
  const [scrollTop, setScrollTop] = useState(0);
  const [contentHeight, setContentHeight] = useState(0);
  const contentRef = useRef<any>(null);
  const containerRef = useRef<any>(null);
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
        setContainerWidth((prev) => (prev === w ? prev : w));
      }
      if (contentRef.current) {
        const { height: measured } = measureElement(contentRef.current);
        setContentHeight((prev) => (prev === measured ? prev : measured));
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

  // #13: force a jump to the end when the caller changes scrollToEndKey. The
  // content for the new selection may not be measured yet, so we latch a
  // pending flag and re-apply once contentHeight settles (see effect below).
  const pendingScrollToEnd = useRef(false);
  const prevScrollToEndKey = useRef(scrollToEndKey);
  useEffect(() => {
    if (scrollToEndKey === prevScrollToEndKey.current) return;
    prevScrollToEndKey.current = scrollToEndKey;
    if (scrollToEndKey === undefined) return;
    pendingScrollToEnd.current = true;
    userScrolledUp.current = false;
    setScrollTop(maxScrollRef.current);
  }, [scrollToEndKey]);

  // Re-apply the pending jump after the new selection's content is measured.
  useEffect(() => {
    if (!pendingScrollToEnd.current) return;
    pendingScrollToEnd.current = false;
    setScrollTop(maxScrollRef.current);
  }, [contentHeight]);

  // Reset scroll when height changes
  const prevHeight = useRef(height);
  useEffect(() => {
    if (height !== prevHeight.current) {
      prevHeight.current = height;
      setScrollTop((s) => Math.min(s, maxScrollRef.current));
    }
  }, [height]);

  useKeypress(
    (input, key) => {
      if (input === 'k') scroll(-1);
      else if (input === 'j') scroll(1);
      else if (key.pageUp || (key.ctrl && input === 'u'))
        scroll(-Math.floor(height / 2));
      else if (key.pageDown || (key.ctrl && input === 'd'))
        scroll(Math.floor(height / 2));
    },
    { isActive }
  );

  useMouse(
    useCallback(
      (event: { type: string }) => {
        if (event.type === 'scrollup') scroll(-3);
        else if (event.type === 'scrolldown') scroll(3);
      },
      [scroll]
    )
  );

  const showScrollbar = maxScroll > 0;

  return (
    <Box
      ref={containerRef as any}
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
