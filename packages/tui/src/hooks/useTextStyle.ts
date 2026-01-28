import { useTheme } from './useThemeContext.js';
import type { Theme } from '../types/themeTypes.js';

export function useTextStyle(styleName: keyof Theme['textStyles']) {
  const { getColor, textStyles } = useTheme();
  const style = textStyles[styleName];

  // Start with the color chalk chain from getColor (reuses all the terminal detection logic)
  let chalkChain = getColor(style.color);

  // Apply weight modifiers
  if (style.weight === 'bold') {
    chalkChain = chalkChain.bold;
  } else if (style.weight === 'dim') {
    chalkChain = chalkChain.dim;
  }

  // Apply style modifiers
  if (style.style === 'italic') {
    chalkChain = chalkChain.italic;
  }

  // Apply decoration modifiers
  if (style.decoration) {
    style.decoration.forEach((dec) => {
      if (dec === 'underline') {
        chalkChain = chalkChain.underline;
      } else if (dec === 'strikethrough') {
        chalkChain = chalkChain.strikethrough;
      }
    });
  }

  return chalkChain; // Returns the complete chalk chain function
}

// Convenience hook for the label style specifically
export function useLabelStyle() {
  return useTextStyle('label');
}
