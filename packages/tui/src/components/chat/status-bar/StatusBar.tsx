import { Box, measureElement } from 'ink';
import React, { createContext, useCallback, useContext, useLayoutEffect, useMemo, useRef, useState } from 'react';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { Text } from '../../ui/text/Text.js';
import { Icon, IconType } from '../../ui/icon/Icon.js';
import { useCardContext } from '../../ui/card/Card.js';
import { getStatusColor } from '../../../utils/colorUtils.js';
import type { StatusType } from '../../../types/componentTypes.js';

interface StatusBarContextType {
  setLineColor: (lineIndex: number, color: string) => void;
  getNextLineIndex: (count?: number) => number;
  requestRemeasure: () => void;
  setStatus: (status: StatusType | undefined) => void;
  status?: StatusType;
}

const StatusBarContext = createContext<StatusBarContextType | null>(null);

export const useStatusBar = () => {
  const context = useContext(StatusBarContext);
  if (!context) {
    throw new Error('useStatusBar must be used within a StatusBar');
  }
  return context;
};

export interface StatusBarProps {
  children: React.ReactNode;
  /** Default bar color - defaults to brand color */
  barColor?: string;
  /** Status icon to show on first line */
  status?: StatusType;
}

export const StatusBar = React.memo(function StatusBar({ 
  children, 
  barColor: barColorProp, 
  status: statusProp 
}: StatusBarProps) {
  const { getColor } = useTheme();
  const { active } = useCardContext();
  const contentRef = useRef<any>(null);
  const [lineCount, setLineCount] = useState(0);
  const [lineColors, setLineColors] = useState<Map<number, string>>(new Map());
  const [statusOverride, setStatusOverride] = useState<StatusType | undefined>(undefined);
  const currentLineIndexRef = useRef(0);

  // Child-set status takes precedence over prop
  const status = statusOverride ?? statusProp;

  const defaultBarColor = barColorProp || getColor('brand').hex;

  // Set color for a specific line
  const setLineColor = useCallback((lineIndex: number, color: string) => {
    setLineColors(prev => {
      const newMap = new Map(prev);
      newMap.set(lineIndex, color);
      return newMap;
    });
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
    setLineCount(0); // Reset to force re-render with new measurement
    setRemeasureKey(k => k + 1);
    setLineColors(new Map());
  }, []);

  // Reset line index on each render cycle
  currentLineIndexRef.current = 0;

  // Measure component height
  useLayoutEffect(() => {
    if (contentRef.current) {
      const measurement = measureElement(contentRef.current);
      const newHeight = measurement.height;
      if (newHeight > 0) {
        setLineCount(newHeight);
      }
    }
  }, [remeasureKey, children]);

  const contextValue = useMemo(() => ({
    setLineColor,
    getNextLineIndex,
    requestRemeasure,
    setStatus,
    status,
  }), [setLineColor, getNextLineIndex, requestRemeasure, setStatus, status]);

  // Determine if status should show a dot on first line (not for 'active')
  const showDot = status && status !== 'active';

  // Render the status bar column
  const renderedBar = useMemo(() => {
    if (lineCount === 0) return null;
    
    const barElements = [];
    for (let i = 0; i < lineCount; i++) {
      // First line gets icon for certain status types
      if (i === 0 && showDot) {
        barElements.push(
          <Box key={i}>
            <Icon type={IconType.DOT} color={getStatusColor(status!, getColor)} />
          </Box>,
        );
      } else if (active) {
        // Use line-specific override color, or status color, or default
        const color = lineColors.get(i) || (status ? getStatusColor(status, getColor).hex : defaultBarColor);
        barElements.push(
          <Text key={i} backgroundColor={color}>
            {' '}
          </Text>,
        );
      } else {
        // Empty space for inactive cards
        barElements.push(<Text key={i}> </Text>);
      }
    }
    return (
      <Box flexDirection="column" width={1}>
        {barElements}
      </Box>
    );
  }, [lineCount, status, showDot, active, lineColors, defaultBarColor, getColor]);

  return (
    <StatusBarContext.Provider value={contextValue}>
      <Box flexDirection="row" width="100%">
        {renderedBar}
        <Box flexDirection="column" flexGrow={1} marginLeft={renderedBar ? 1 : 0} ref={contentRef}>
          {children}
        </Box>
      </Box>
    </StatusBarContext.Provider>
  );
});