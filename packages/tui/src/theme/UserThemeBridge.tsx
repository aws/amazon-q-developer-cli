/**
 * Bridge component that connects ThemeProvider's setUserColors to the Zustand store.
 * Must be rendered inside both ThemeProvider and AppStoreProvider.
 */

import { useEffect } from 'react';
import { useTheme } from '../hooks/useThemeContext.js';
import { useAppStore } from '../stores/app-store.js';

export const UserThemeBridge = () => {
  const { setUserColors } = useTheme();
  const registerUserColorsSetter = useAppStore(
    (state) => state.registerUserColorsSetter
  );

  useEffect(() => {
    registerUserColorsSetter(setUserColors);
  }, [setUserColors, registerUserColorsSetter]);

  return null;
};
