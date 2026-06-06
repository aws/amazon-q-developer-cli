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
 * Renders the synthetic-scrollback preview pane shown beneath the /verbosity
 * menu. Reads live config every render so toggling a knob (which re-opens
 * the same submenu) reflects in the next frame.
 *
 * `which` selects the fixture set (top/density/tool/subagent/output/
 * truncation:args/truncation:output). For the truncation editor's in-progress
 * value, the editor mounts its own preview by passing draft caps via
 * `displayOverride`. The density menu uses both overrides to draft-preview
 * a highlighted preset before the user commits.
 */
export const VerbosityPreview: React.FC<{
  which: VerbosityPreviewKey;
  /**
   * Optional draft display override — used by the truncation editor to
   * preview a not-yet-committed cap value, and by the density menu to
   * draft-preview a highlighted preset. When omitted, current config is
   * read from disk via getVerboseDisplay().
   */
  displayOverride?: ReturnType<typeof getVerboseDisplay>;
  /**
   * Optional draft filter override — paired with displayOverride for the
   * density menu's draft preview, since picking a preset rewrites filters
   * too. When omitted, filters are read from disk.
   */
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
