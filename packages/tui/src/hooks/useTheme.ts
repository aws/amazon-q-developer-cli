import { useAppStore } from '../stores/app-store';
import { type Theme } from '../types/theme';

export const useTheme = () => {
  const theme = useAppStore((state) => state.theme);
  const setTheme = useAppStore((state) => state.setTheme);

  return {
    theme,
    setTheme,
    colors: theme.colors,
  };
};
