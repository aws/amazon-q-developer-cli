import { useMemo } from 'react';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { buildRenderTheme } from '../../../lite/render.js';

// Build the RenderTheme from live theme colors so the preview matches
// scrollback under the user's theme (not hardcoded purple/cyan).
export function useRenderTheme() {
  const { getColor, getUserPromptColor, getUserPromptBgHex } = useTheme();
  const theme = useMemo(
    () => buildRenderTheme(getColor, getUserPromptColor, getUserPromptBgHex),
    [getColor, getUserPromptColor, getUserPromptBgHex]
  );
  const dim = useMemo(() => getColor('secondary'), [getColor]);
  return { theme, dim };
}
