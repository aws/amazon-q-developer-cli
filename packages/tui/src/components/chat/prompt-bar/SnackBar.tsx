import React, { useState, useEffect } from 'react';
import { Box } from 'ink';
import { Text } from '../../ui/text/Text.js';
import { useTheme } from '../../../hooks/useThemeContext.js';

export interface Action {
  key: string;
  label: string;
  description?: string;
}

export interface SnackBarProps {
  title: string;
  actions: Action[];
  width?: number;
  slideIn?: boolean;
}

export function SnackBar({ title, actions, width, slideIn = false }: SnackBarProps) {
  const { getColor } = useTheme();
  const [currentHeight, setCurrentHeight] = useState(slideIn ? 1 : 3);
  const [showText, setShowText] = useState(!slideIn);

  const textColor = getColor('components.snackbar.text');
  const textBold = getColor('components.snackbar.text').bold;

  // Slide in animation effect
  useEffect(() => {
    if (slideIn) {
      // Start with height 1, background visible
      setCurrentHeight(0);
      setShowText(false);

      // Animation sequence - slower for visibility
      const timer1 = setTimeout(() => setCurrentHeight(1), 250); // Grow to height
      const timer2 = setTimeout(() => setCurrentHeight(2), 350);
      const timer3 = setTimeout(() => setCurrentHeight(3), 450);
      const timer4 = setTimeout(() => setShowText(true), 500); // Show text

      return () => {
        clearTimeout(timer1);
        clearTimeout(timer2);
        clearTimeout(timer3);
        clearTimeout(timer4);
      };
    }
  }, [slideIn]);

  // Format each action with proper styling
  const formatAction = (action: Action) => {
    const keyIndex = action.label.toLowerCase().indexOf(action.key.toLowerCase());
    let result = '';

    if (keyIndex !== -1) {
      // Key found in label - split and style
      const before = action.label.slice(0, keyIndex);
      const keyChar = action.label[keyIndex];
      const after = action.label.slice(keyIndex + 1);

      result =
        textColor(before) + textColor('(') + textBold(keyChar) + textColor(')') + textColor(after);
    } else {
      // Key not found - prepend it
      result = textColor('(') + textBold(action.key) + textColor(')') + textColor(action.label);
    }

    if (action.description) {
      result += textColor(` ${action.description}`);
    }

    return result;
  };

  const formattedActions = actions.map(formatAction).join(textColor(', '));
  const content = textColor(title) + textColor(' | ') + formattedActions;

  return (
    <Box
      height={currentHeight}
      width={width || '100%'}
      backgroundColor={getColor('components.snackbar.background').hex}
      paddingX={1}
      paddingY={0}
      flexDirection="column"
      justifyContent="center"
    >
      <Box width="100%" flexShrink={1}>
        {showText && <Text>{content}</Text>}
      </Box>
    </Box>
  );
}
