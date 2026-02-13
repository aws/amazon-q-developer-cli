import { useContext } from 'react';
import {
  ThemeContext,
  type ThemeContextValue,
} from '../theme/ThemeProvider.js';

/**
 * Custom hook to access the current theme context.
 * Provides access to theme values and the getColor helper function.
 *
 * @returns The current theme context value with color utilities
 */
export const useTheme = (): ThemeContextValue => useContext(ThemeContext);
