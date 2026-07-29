import React from 'react';
import { Box } from './../../renderer.js';
import { Text } from './text/Text.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { Menu, type MenuItem } from './menu/Menu.js';
import { Panel } from './panel/Panel.js';
import { useKeypress } from '../../hooks/useKeypress.js';
import type { VoiceModelDownloadInfo } from '../../stores/app-store.js';

interface VoiceModelDownloadGateProps {
  info: VoiceModelDownloadInfo;
  onConfirm: () => void;
  onDecline: () => void;
}

const YES = '(y) Yes, download the model';
const NO = '(n) No, not now';

/**
 * First-use confirm gate for the voice speech-model download. Rendered in place
 * of the prompt input while active; the Menu owns the keyboard (arrow/Enter and
 * Esc), so keystrokes never leak into the chat prompt.
 */
export const VoiceModelDownloadGate: React.FC<VoiceModelDownloadGateProps> = ({
  info,
  onConfirm,
  onDecline,
}) => {
  const { getColor } = useTheme();
  const secondary = getColor('secondary');
  const primary = getColor('primary');

  // One-shot guard: keypress (y/n) and the Menu (Enter/Esc) can both resolve
  // the gate. Fire the response exactly once so a fast double-press or an
  // Enter+keypress race can't kick off the download twice or leave the gate
  // half-dismissed.
  const respondedRef = React.useRef(false);
  const confirmOnce = () => {
    if (respondedRef.current) return;
    respondedRef.current = true;
    onConfirm();
  };
  const declineOnce = () => {
    if (respondedRef.current) return;
    respondedRef.current = true;
    onDecline();
  };

  const items: MenuItem[] = [
    { label: YES, description: `~${info.sizeMb}MB, one-time` },
    { label: NO, description: '' },
  ];

  const handleSelect = (item: MenuItem) => {
    if (item.label === YES) confirmOnce();
    else declineOnce();
  };

  // Direct y/n shortcuts (parity with tool-approval prompts): y = download,
  // n = decline. Arrow+Enter via the Menu still works.
  useKeypress((input) => {
    if (input === 'y' || input === 'Y') confirmOnce();
    else if (input === 'n' || input === 'N') declineOnce();
  });

  return (
    <Panel
      title="Voice setup — download speech model?"
      onClose={declineOnce}
      showTabHint={false}
      hideTitleDivider={true}
    >
      <Box flexDirection="column">
        <Box marginBottom={1}>
          <Text wrap="wrap">
            {primary(
              `Voice input uses the OpenAI Whisper model (${info.model}, ~${info.sizeMb}MB). It downloads once and runs locally.`
            )}
          </Text>
        </Box>
        <Box marginBottom={1}>
          <Text wrap="wrap">
            {secondary(`${info.license} license — ${info.licenseUrl}`)}
          </Text>
        </Box>
        <Menu
          items={items}
          onSelect={handleSelect}
          onEscape={declineOnce}
          showSelectedIndicator={true}
        />
      </Box>
    </Panel>
  );
};
