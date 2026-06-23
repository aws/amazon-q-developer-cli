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
import { getVerboseConfig, getVerboseDisplay } from '../../../lite/verbose.js';

/**
 * Inline synthetic-scrollback preview beneath the /verbosity menu. Reads live
 * config every render so toggling a knob reflects on the next frame.
 *
 * `displayOverride`/`filtersOverride` are draft overrides for an in-progress
 * truncation cap or highlighted density preset; both default to saved config.
 */
export const VerbosityPreview: React.FC<{
  which: VerbosityPreviewKey;
  displayOverride?: ReturnType<typeof getVerboseDisplay>;
  filtersOverride?: readonly string[];
}> = ({ which, displayOverride, filtersOverride }) => {
  const { getColor, getUserPromptColor, getUserPromptBgHex } = useTheme();
  const secondary = useMemo(() => getColor('secondary'), [getColor]);
  // Per-render theme so the preview matches scrollback under the user's theme
  // (not hardcoded purple/cyan).
  const theme = useMemo(
    () => buildRenderTheme(getColor, getUserPromptColor, getUserPromptBgHex),
    [getColor, getUserPromptColor, getUserPromptBgHex]
  );

  const display = displayOverride ?? getVerboseDisplay();
  const filters = filtersOverride ?? getVerboseConfig().filters;

  const preview = useMemo(
    () => renderVerbosityPreview(which, display, filters, { theme }),
    [which, display, filters, theme]
  );

  if (!preview) return null;

  return (
    <Box flexDirection="column" marginTop={1}>
      <Divider />
      <Box paddingX={1} flexDirection="column">
        <Text>{secondary('Preview')}</Text>
        <Text>{preview}</Text>
      </Box>
    </Box>
  );
};
