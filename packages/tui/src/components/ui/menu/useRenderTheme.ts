import { useMemo } from 'react';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { buildRenderTheme } from '../../../lite/render.js';

/**
 * Per-render theme + dim color for the verbosity preview surfaces. The preview
 * must match what scrollback looks like under the user's theme, so it builds a
 * RenderTheme from the live theme colors rather than hardcoding purple/cyan.
 */
export function useRenderTheme() {
  const { getColor, getUserPromptColor, getUserPromptBgHex } = useTheme();
  const theme = useMemo(
    () => buildRenderTheme(getColor, getUserPromptColor, getUserPromptBgHex),
    [getColor, getUserPromptColor, getUserPromptBgHex]
  );
  const dim = useMemo(() => getColor('secondary'), [getColor]);
  return { theme, dim };
}
