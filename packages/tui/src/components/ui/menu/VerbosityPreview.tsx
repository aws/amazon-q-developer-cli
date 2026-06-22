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
 */
export const VerbosityPreview: React.FC<{
  which: VerbosityPreviewKey;
  /** Draft overrides for an in-progress truncation cap / highlighted density
   *  preset; default to saved config read from disk. */
  displayOverride?: ReturnType<typeof getVerboseDisplay>;
  filtersOverride?: readonly string[];
}> = ({ which, displayOverride, filtersOverride }) => {
  const { getColor, getUserPromptColor, getUserPromptBgHex } = useTheme();
  const secondary = useMemo(() => getColor('secondary'), [getColor]);
  // Per-render theme: the preview surface needs to match what scrollback
  // looks like with the user's chosen theme, otherwise picking a preset
  // would still show the synthetic preview with hardcoded purple/cyan.
  const theme = useMemo(
    () => buildRenderTheme(getColor, getUserPromptColor, getUserPromptBgHex),
    [getColor, getUserPromptColor, getUserPromptBgHex]
  );

  // Read live config on every render — toggling a knob re-opens the menu,
  // which rerenders this component, which reads the freshly-saved config.
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
