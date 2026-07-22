/** Cloud-entry gate shown when a `--cloud` session has no connected source
 *  provider. A blocking menu (open browser / retry / quit); cloud bring-up
 *  stays parked behind it until a retry finds a connected provider. Over SSH
 *  the browser row is omitted — a browser opened on the remote host could
 *  never reach the user — and a hint points at connecting from another
 *  signed-in device instead. */
import React, { useMemo, useState } from 'react';
import { Box, useInput } from '../../renderer.js';
import { Text } from './text/Text.js';
import { Spinner } from './spinner/Spinner.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useGlyphs } from '../../hooks/useGlyphs.js';
import { chalk } from '../../utils/color.js';
import { SOURCE_PROVIDER_SETUP_URL } from '../../utils/cloud-urls.js';
import { isRemoteEnvironment } from '../../utils/browser.js';

type OptionKey = 'browser' | 'retry' | 'quit';
const ALL_OPTIONS: { key: OptionKey; label: string }[] = [
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
  /** Test override for remote/headless detection (defaults to env probe). */
  isRemote?: boolean;
}

export const SourceProviderGate: React.FC<SourceProviderGateProps> = ({
  setupUrl,
  onOpenBrowser,
  onRetry,
  onQuit,
  isRemote,
}) => {
  const { getColor, colors } = useTheme();
  const glyphs = useGlyphs();
  const dim = getColor('secondary');
  const brand = getColor('brand');
  const primary = getColor('primary');
  const accentHex =
    (colors as { accent?: { truecolor?: string } }).accent?.truecolor ??
    '#ff00ff';

  const envRemote = useMemo(() => isRemoteEnvironment(), []);
  const remote = isRemote ?? envRemote;
  const options = remote
    ? ALL_OPTIONS.filter((o) => o.key !== 'browser')
    : ALL_OPTIONS;

  const [cursor, setCursor] = useState(0);
  const [checking, setChecking] = useState(false);

  useInput((_input, key) => {
    if (checking) return;
    if (key.upArrow) {
      setCursor((i) => Math.max(0, i - 1));
      return;
    }
    if (key.downArrow) {
      setCursor((i) => Math.min(options.length - 1, i + 1));
      return;
    }
    if (key.return) {
      const opt = options[cursor];
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
      {/* Body sentences full-strength; only the remote hint is dimmed. */}
      <Text>{primary('Cloud sessions require a source provider.')}</Text>
      <Text>
        {primary('Connect one on kiro.dev: ')}
        {brand(url)}
      </Text>
      {remote && (
        <>
          <Box height={1} />
          <Text wrap="wrap">
            {dim(
              "Open the URL above on any device where you're signed in, " +
                "connect a source provider, then select 'Refresh and try " +
                "again' below."
            )}
          </Text>
        </>
      )}
      <Box height={1} />

      {checking ? (
        <Box>
          <Spinner />
          <Text>{dim(' Connecting…')}</Text>
        </Box>
      ) : (
        options.map((opt, i) => {
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
