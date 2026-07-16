/** Cloud-entry gate shown when a `--cloud` session has no connected source
 *  provider. A blocking 3-option menu (open browser / retry / quit); cloud
 *  bring-up stays parked behind it until a retry finds a connected provider. */
import React, { useState } from 'react';
import { Box, useInput } from '../../renderer.js';
import { Text } from './text/Text.js';
import { Spinner } from './spinner/Spinner.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useGlyphs } from '../../hooks/useGlyphs.js';
import chalk from 'chalk';
import { SOURCE_PROVIDER_SETUP_URL } from '../../utils/cloud-urls.js';

type OptionKey = 'browser' | 'retry' | 'quit';
const OPTIONS: { key: OptionKey; label: string }[] = [
  { key: 'browser', label: 'Open in browser' },
  { key: 'retry', label: 'Refresh and try again' },
  { key: 'quit', label: 'Quit' },
];

export interface SourceProviderGateProps {
  /** Kiro Web handoff URL; falls back to the settings page when unknown. */
  setupUrl: string | null;
  /** Open the setup URL and keep the gate up. */
  onOpenBrowser: () => void;
  /** Re-probe the provider list; resolves when the check completes. */
  onRetry: () => Promise<void> | void;
  /** Exit the session cleanly. */
  onQuit: () => void;
}

export const SourceProviderGate: React.FC<SourceProviderGateProps> = ({
  setupUrl,
  onOpenBrowser,
  onRetry,
  onQuit,
}) => {
  const { getColor, colors } = useTheme();
  const glyphs = useGlyphs();
  const dim = getColor('secondary');
  const brand = getColor('brand');
  const primary = getColor('primary');
  const accentHex =
    (colors as { accent?: { truecolor?: string } }).accent?.truecolor ??
    '#ff00ff';

  const [cursor, setCursor] = useState(0);
  const [checking, setChecking] = useState(false);

  useInput((_input, key) => {
    if (checking) return;
    if (key.upArrow) {
      setCursor((i) => Math.max(0, i - 1));
      return;
    }
    if (key.downArrow) {
      setCursor((i) => Math.min(OPTIONS.length - 1, i + 1));
      return;
    }
    if (key.return) {
      const opt = OPTIONS[cursor];
      if (!opt) return;
      if (opt.key === 'browser') {
        onOpenBrowser();
      } else if (opt.key === 'quit') {
        onQuit();
      } else {
        // Show a connecting spinner while the re-probe runs; on failure the
        // gate falls back into view (the store leaves it mounted).
        setChecking(true);
        Promise.resolve(onRetry()).finally(() => setChecking(false));
      }
    }
  });

  const url = setupUrl ?? SOURCE_PROVIDER_SETUP_URL;

  return (
    <Box flexDirection="column" paddingX={1}>
      <Text>{primary.bold('Source provider not found')}</Text>
      <Box height={1} />
      <Text>{dim('Cloud sessions require a source provider.')}</Text>
      <Text>
        {dim('Connect one on kiro.dev: ')}
        {brand(url)}
      </Text>
      <Box height={1} />

      {checking ? (
        <Box>
          <Spinner />
          <Text>{dim(' Connecting…')}</Text>
        </Box>
      ) : (
        OPTIONS.map((opt, i) => {
          const isCursor = i === cursor;
          const line = isCursor
            ? chalk.hex(accentHex).bold(`${glyphs.chevron} ${opt.label}`)
            : `  ${opt.label}`;
          return <Text key={opt.key}>{line}</Text>;
        })
      )}

      <Box height={1} />
      <Text>
        {brand(`${glyphs.arrowUp}${glyphs.arrowDown}`)}{' '}
        {dim('up/down to navigate')}
        {dim(` ${glyphs.smallDot} `)}
        {brand(glyphs.enter)} {dim('enter to select')}
      </Text>
    </Box>
  );
};
