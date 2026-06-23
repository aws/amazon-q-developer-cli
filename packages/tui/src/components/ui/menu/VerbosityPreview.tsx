import React, { useMemo } from 'react';
import { Box } from '../../../renderer.js';
import { Text } from '../text/Text.js';
import { Divider } from '../divider/Divider.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import {
  renderVerbosityPreview,
  buildRenderTheme,
  type VerbosityPreviewKey,
} from '../../../lite/render.js';
import {
  getVerboseConfig,
  getVerboseDisplay,
  type VerboseDisplayConfig,
} from '../../../lite/verbose.js';

/**
 * Resolve preview text + the dim chalk fn from live config every render, so
 * toggling a knob reflects on the next frame. `displayOverride`/`filtersOverride`
 * are draft overrides for an in-progress truncation cap or highlighted density
 * preset; both default to saved config. Shared by the inline preview and the
 * expanded pane.
 */
export function useVerbosityPreviewText(
  which: VerbosityPreviewKey,
  displayOverride: VerboseDisplayConfig | undefined,
  filtersOverride: readonly string[] | undefined,
  expanded = false
): { text: string; dim: (s: string) => string } {
  const { getColor, getUserPromptColor, getUserPromptBgHex } = useTheme();
  const dim = useMemo(() => getColor('secondary'), [getColor]);
  // Per-render theme so the preview matches scrollback under the user's theme
  // (not hardcoded purple/cyan).
  const theme = useMemo(
    () => buildRenderTheme(getColor, getUserPromptColor, getUserPromptBgHex),
    [getColor, getUserPromptColor, getUserPromptBgHex]
  );

  const display = displayOverride ?? getVerboseDisplay();
  const filters = filtersOverride ?? getVerboseConfig().filters;

  const text = useMemo(
    () => renderVerbosityPreview(which, display, filters, { expanded, theme }),
    [which, display, filters, expanded, theme]
  );

  return { text, dim };
}

export const VerbosityPreview: React.FC<{
  which: VerbosityPreviewKey;
  displayOverride?: VerboseDisplayConfig;
  filtersOverride?: readonly string[];
}> = ({ which, displayOverride, filtersOverride }) => {
  const { text, dim } = useVerbosityPreviewText(
    which,
    displayOverride,
    filtersOverride
  );

  if (!text) return null;

  return (
    <Box flexDirection="column" marginTop={1}>
      <Divider />
      <Box paddingX={1} flexDirection="column">
        <Text>{dim('Preview')}</Text>
        <Text>{text}</Text>
      </Box>
    </Box>
  );
};
