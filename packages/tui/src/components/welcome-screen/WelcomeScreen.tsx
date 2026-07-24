import React from 'react';
import { Box, Text as InkText } from './../../renderer.js';
import { Wordmark /*useTheme*/ } from '../brand/index.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useGlyphs } from '../../hooks/useGlyphs.js';
import { Text } from '../ui/text/Text.js';
import { useAppStore } from '../../stores/app-store.js';

export interface WelcomeScreenProps {
  agent: string;
  mcpServers: string[];
  animate?: boolean;
  /**
   * Rotating startup tip, already selected by the caller (see tips/tips.ts).
   * Rendered centered below the welcome content with a bold "Tip:" prefix (à la
   * V1's "Did you know?" box) so it reads as meta-help, not chat. Persisted:
   * the caller passes it on the <Static> re-render too, so it stays in
   * scrollback rather than vanishing after the first message.
   */
  tip?: string;
}

/** A keyboard shortcut token: a modifier combo (Ctrl+C, Shift+←/→) or a bare Esc. */
const KEYBIND = /^(?:Esc|(?:Ctrl|Shift|Alt|Cmd|Opt)\+\S*)$/;

/**
 * Color a line word-by-word: slash-commands, `--flags`, and keybind tokens
 * brand (purple), rest primary.
 */
function colorize(
  line: string,
  primary: (s: string) => string,
  brand: (s: string) => string
): string {
  return line
    .split(' ')
    .map((word) =>
      word.startsWith('/') || word.startsWith('--') || KEYBIND.test(word)
        ? brand(word)
        : primary(word)
    )
    .join(' ');
}

export const WelcomeScreen = React.memo(function WelcomeScreen({
  // agent,
  // mcpServers,
  animate = false,
  tip,
}: WelcomeScreenProps) {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const primary = getColor('primary');
  const brand = getColor('brand');
  const dim = getColor('muted');
  const agentEngine = useAppStore((s) => s.agentEngine);
  const isKas = agentEngine === 'kas';

  // Shared "what's new" body rendered under both the V2 and V3 headings. The
  // /feedback nudge that used to live here is carried by the rotating tip pool
  // now (tips/tips.ts), so it isn't duplicated as a persistent line.
  const whatsNewBody = (
    <>
      <Text>{primary(' ')}</Text>
      <Text>
        {primary.bold("What's new:")}
        {primary(' Specs, expanded hooks, and an improved trust model.')}
      </Text>
      {isKas && (
        <Text>
          {primary('Upgrade your V2 agent configurations to V3 with ')}
          {primary.bold('/upgrade-agent')}
        </Text>
      )}
      <Text>{brand('https://kiro.dev/docs/cli/v3/')}</Text>
    </>
  );

  return (
    <Box flexDirection="column" width="100%" alignItems="center">
      <Wordmark animate={animate} />
      {process.env.NODE_ENV !== 'production' && (
        <InkText
          dimColor
        >{`Development Mode ${glyphs.smallDot} Twinki`}</InkText>
      )}

      <Box
        flexDirection="column"
        alignItems="center"
        marginTop={1}
        paddingX={2}
      >
        {isKas ? (
          <Text>
            {primary('Welcome to ')}
            {brand('Kiro CLI V3')}
            {primary('!')}
          </Text>
        ) : (
          <Text>
            {primary('An early release of ')}
            {brand('Kiro CLI V3')}
            {primary(' is now available! Try it out: ')}
            {brand('kiro-cli --v3')}
          </Text>
        )}
        {whatsNewBody}
        {tip && (
          <>
            <Text>{primary(' ')}</Text>
            <Text>
              {primary.bold('Tip: ')}
              {colorize(tip, primary, brand)}
            </Text>
          </>
        )}
      </Box>
    </Box>
  );
});
