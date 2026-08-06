import { Box, measureElement } from './../../../renderer.js';
import React, {
  createContext,
  useCallback,
  useContext,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useDropsLeftStatusBar } from '../../../hooks/useDropsLeftStatusBar.js';
import { Text } from '../../ui/text/Text.js';
import { Icon, IconType } from '../../ui/icon/Icon.js';
import { Spinner } from '../../ui/spinner/Spinner.js';
import { PieSpinner } from '../../ui/spinner/PieSpinner.js';
import { useCardContext } from '../../ui/card/Card.js';
import {
  getStatusColor,
  getTerminalChalkColor,
} from '../../../utils/colorUtils.js';
import type { StatusType } from '../../../types/componentTypes.js';

/** Width of the vertical accent bar column (characters). */
export const STATUS_BAR_WIDTH = 1;
/** Gap between the bar and content (characters). */
export const STATUS_BAR_MARGIN_LEFT = 1;
/**
 * Total left offset before content begins. Use this to align elements
 * rendered outside a StatusBar with its content zone.
 */
export const STATUS_BAR_CONTENT_OFFSET =
  STATUS_BAR_WIDTH + STATUS_BAR_MARGIN_LEFT;

interface StatusBarContextType {
  setLineColor: (lineIndex: number, color: string) => void;
  setLineColors: (colors: Record<number, string>) => void;
  getNextLineIndex: (count?: number) => number;
  requestRemeasure: () => void;
  setStatus: (status: StatusType | undefined) => void;
  status?: StatusType;
}

const StatusBarContext = createContext<StatusBarContextType | null>(null);

export const useStatusBar = () => {
  const context = useContext(StatusBarContext);
  if (!context) {
    // No-op fallback when rendered outside a StatusBar (e.g. wrapDisabled
    // mode that skips the chrome for scrollback content). Consumers that
    // observe measurement changes (`requestRemeasure`) get a silent no-op;
    // there's nothing to remeasure when there's no StatusBar wrapper.
    return {
      setLineColor: () => {},
      setLineColors: () => {},
      getNextLineIndex: (count: number = 1) => {
        void count;
        return 0;
      },
      requestRemeasure: () => {},
      setStatus: () => {},
      status: undefined,
    } as StatusBarContextType;
  }
  return context;
};

export interface StatusBarProps {
  children: React.ReactNode;
  /** Default bar color - defaults to brand color */
  barColor?: string;
  /** Status icon to show on first line */
  status?: StatusType;
  /**
   * Drop the left accent-bar gutter entirely and render children flush-left.
   * Used by surfaces (e.g. the session/agent-monitor output panes) that embed
   * StatusBar content but don't want the chat accent chrome or its 2-char
   * left offset. The status-icon column is still preserved when a status is
   * set so dots/spinners remain visible.
   */
  noBar?: boolean;
}

export const StatusBar = React.memo(function StatusBar({
  children,
  barColor: barColorProp,
  status: statusProp,
  noBar = false,
}: StatusBarProps) {
  // Checked here rather than at each call site so no message type can
  // reintroduce the bar on a surface that drops it.
  const dropsBar = useDropsLeftStatusBar();
  if (dropsBar) {
    return <>{children}</>;
  }
  return (
    <StatusBarChrome barColor={barColorProp} status={statusProp} noBar={noBar}>
      {children}
    </StatusBarChrome>
  );
});

const StatusBarChrome = React.memo(function StatusBarChrome({
  children,
  barColor: barColorProp,
  status: statusProp,
  noBar = false,
}: StatusBarProps) {
  const { getColor } = useTheme();
  const { active } = useCardContext();
  const contentRef = useRef<any>(null);
  const [lineCount, setLineCount] = useState(0);
  const [lineColors, setLineColors] = useState<Record<number, string>>({});
  const [statusOverride, setStatusOverride] = useState<StatusType | undefined>(
    undefined
  );
  const currentLineIndexRef = useRef(0);

  // Child-set status takes precedence over prop
  const status = statusOverride ?? statusProp;

  const defaultBarColor = barColorProp || getColor('brand').hex;

  // Set color for a specific line
  const setLineColor = useCallback((lineIndex: number, color: string) => {
    setLineColors((prev) => ({ ...prev, [lineIndex]: color }));
  }, []);

  // Batch set colors for multiple lines at once
  const setLineColorsBatch = useCallback((colors: Record<number, string>) => {
    setLineColors((prev) => ({ ...prev, ...colors }));
  }, []);

  // Get next line index and advance counter
  const getNextLineIndex = useCallback((count: number = 1) => {
    const index = currentLineIndexRef.current;
    currentLineIndexRef.current += count;
    return index;
  }, []);

  // Allow children to override status
  const setStatus = useCallback((newStatus: StatusType | undefined) => {
    setStatusOverride(newStatus);
  }, []);

  // Allow children to request a remeasure
  const [remeasureKey, setRemeasureKey] = useState(0);
  const requestRemeasure = useCallback(() => {
    setRemeasureKey((k) => k + 1);
  }, []);

  // Reset line index on each render cycle
  currentLineIndexRef.current = 0;

  // Measure component height after layout.
  // remeasureKey is bumped by children (via requestRemeasure) to force a
  // measurement pass — needed because Yoga layout may not be ready on the
  // first commit after content changes.
  useLayoutEffect(() => {
    if (contentRef.current) {
      const { height } = measureElement(contentRef.current);
      if (height > 0) {
        setLineCount(height);
      }
    }
  }, [remeasureKey, children]);

  const contextValue = useMemo(
    () => ({
      setLineColor,
      setLineColors: setLineColorsBatch,
      getNextLineIndex,
      requestRemeasure,
      setStatus,
      status,
    }),
    [
      setLineColor,
      setLineColorsBatch,
      getNextLineIndex,
      requestRemeasure,
      setStatus,
      status,
    ]
  );

  // Determine if status should show a dot on first line (not for 'active', 'thinking', or 'paused')
  const showDot =
    status &&
    status !== 'active' &&
    status !== 'thinking' &&
    status !== 'executing' &&
    status !== 'paused' &&
    status !== 'usage';
  const showSpinner = status === 'thinking';
  const showPieSpinner = status === 'executing';
  const showArrowDown = status === 'paused';
  const showArrowRight = status === 'usage';
  const hasStatusIcon =
    showDot || showSpinner || showPieSpinner || showArrowDown || showArrowRight;

  // Render the status bar column elements
  const barElements = useMemo(() => {
    // When lineCount is 0 but we have a status to show (e.g. inside Ink's <Static>
    // where measureElement doesn't trigger), render at least the status icon
    const effectiveLineCount = lineCount === 0 && hasStatusIcon ? 1 : lineCount;

    if (effectiveLineCount === 0) return null;

    const elements = [];
    for (let i = 0; i < effectiveLineCount; i++) {
      // First line: pie spinner for executing, braille spinner for thinking, arrow for paused, dot for others
      if (i === 0 && showPieSpinner) {
        const pieColor = barColorProp
          ? getTerminalChalkColor({ truecolor: barColorProp })
          : getColor('brand');
        elements.push(
          <Box key={i}>
            <PieSpinner color={pieColor} />
          </Box>
        );
      } else if (i === 0 && showSpinner) {
        const spinnerColor = barColorProp
          ? getTerminalChalkColor({ truecolor: barColorProp })
          : getStatusColor('thinking', getColor);
        elements.push(
          <Box key={i}>
            <Spinner color={spinnerColor} />
          </Box>
        );
      } else if (i === 0 && showArrowDown) {
        elements.push(
          <Box key={i}>
            <Icon
              type={IconType.ARROW_DOWN}
              color={getStatusColor('paused', getColor)}
            />
          </Box>
        );
      } else if (i === 0 && showArrowRight) {
        elements.push(
          <Box key={i}>
            <Icon type={IconType.ARROW_RIGHT} color={getColor('secondary')} />
          </Box>
        );
      } else if (i === 0 && showDot) {
        const dotColor = getStatusColor(status!, getColor);
        elements.push(
          <Box key={i}>
            <Icon type={IconType.DOT} color={dotColor} />
          </Box>
        );
      } else if (
        active &&
        status !== 'paused' &&
        status !== 'usage' &&
        !noBar
      ) {
        // Use line-specific override color, or barColor prop, or status color, or default
        // Don't show bar for paused status (only show the arrow icon).
        // When noBar is set we keep the line-0 status icon but never paint the
        // solid accent block on continuation lines (that block was the leftover
        // green/colored gutter visible on monitor tool cards).
        const color =
          lineColors[i] ||
          (status && status !== 'active'
            ? getStatusColor(status, getColor).hex
            : defaultBarColor);
        elements.push(
          <Text key={i} backgroundColor={color}>
            {' '}
          </Text>
        );
      } else {
        // Empty space for inactive cards or paused status
        elements.push(<Text key={i}> </Text>);
      }
    }
    return elements;
  }, [
    lineCount,
    status,
    showDot,
    showSpinner,
    showArrowDown,
    showArrowRight,
    showPieSpinner,
    hasStatusIcon,
    active,
    lineColors,
    defaultBarColor,
    getColor,
    barColorProp,
    noBar,
  ]);

  // Background color for the bar column — fills any gap between bar elements and content height
  const barBgColor = useMemo(() => {
    if (
      !active ||
      status === 'paused' ||
      status === 'usage' ||
      status === 'thinking' ||
      status === 'executing'
    )
      return undefined;
    if (status && status !== 'active')
      return getStatusColor(status, getColor).hex;
    return defaultBarColor;
  }, [active, status, defaultBarColor, getColor]);

  // When noBar is set, drop the accent-bar gutter and its left offset so
  // children render flush-left. Keep the status-icon column only when there's
  // an icon to show (dot/spinner/arrow), so status affordances survive.
  const hideGutter = noBar && !hasStatusIcon;

  return (
    <StatusBarContext.Provider value={contextValue}>
      <Box flexDirection="row" width="100%">
        {/* Bar stretches to match content; content sizes to its own height */}
        {!hideGutter && (
          <Box
            flexDirection="column"
            width={STATUS_BAR_WIDTH}
            justifyContent="flex-start"
            backgroundColor={noBar ? undefined : barBgColor}
          >
            {barElements}
          </Box>
        )}
        <Box
          flexDirection="column"
          flexGrow={1}
          flexShrink={1}
          alignSelf="flex-start"
          marginLeft={hideGutter ? 0 : STATUS_BAR_MARGIN_LEFT}
          ref={contentRef}
        >
          {children}
        </Box>
      </Box>
    </StatusBarContext.Provider>
  );
});
