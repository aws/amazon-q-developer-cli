import { createContext, type ReactNode } from 'react';
import { kiroDark } from './kiroDark';
import { kiroLight } from './kiroLight';
import type { Theme } from './types';
import { getTerminalChalkColor } from '../utils/colorUtils';
import { detectTerminalTheme } from '../utils/terminal-theme';

/**
 * Extended theme context value that includes a helper function for getting terminal colors.
 * This interface combines the base Theme with a convenient color getter method.
 */
export interface ThemeContextValue extends Theme {
  getColor: (colorPath: string) => any; // Returns chalk chain that can be called or further chained
}

/**
 * Creates a theme context value with enhanced color functionality.
 * Transforms a basic Theme into a ThemeContextValue by adding the getColor helper method.
 *
 * @param theme - The base theme configuration
 * @returns Enhanced theme context with color getter functionality
 */
const createThemeContext = (theme: Theme): ThemeContextValue => ({
  ...theme,
  getColor: (colorPath: string) => {
    const keys = colorPath.split('.');
    let colorDef: any = theme.colors;

    for (const key of keys) {
      colorDef = colorDef[key];
      if (!colorDef) {
        throw new Error(`Color path '${colorPath}' not found in theme`);
      }
    }

    return getTerminalChalkColor(
      colorDef.truecolor,
      colorDef.color256,
      colorDef.named
    );
  },
});

/**
 * Gets the appropriate theme based on terminal/OS appearance detection.
 */
const getAutoTheme = (): Theme => {
  const appearance = detectTerminalTheme();
  return appearance === 'light' ? kiroLight : kiroDark;
};

// Create the React context with the default theme as the initial value
export const ThemeContext = createContext<ThemeContextValue>(
  createThemeContext(kiroDark)
);

/**
 * Props for the ThemeProvider component
 */
interface ThemeProviderProps {
  theme?: Theme | 'auto'; // Optional theme override, 'auto' for detection, defaults to auto-detection
  children: ReactNode;
}

/**
 * ThemeProvider component that provides theme context to child components.
 * Wraps children with theme context, allowing them to access theme values and color utilities.
 *
 * @param theme - Optional theme override ('auto' for detection, or a specific Theme). Defaults to 'auto'.
 * @param children - Child components that will have access to the theme context
 */
export const ThemeProvider = ({
  theme = 'auto',
  children,
}: ThemeProviderProps) => {
  const resolvedTheme = theme === 'auto' ? getAutoTheme() : theme;
  const themeContext = createThemeContext(resolvedTheme);
  return (
    <ThemeContext.Provider value={themeContext}>
      {children}
    </ThemeContext.Provider>
  );
};
