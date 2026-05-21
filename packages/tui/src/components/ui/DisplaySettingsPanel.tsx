import React, { useState, useCallback } from 'react';
import { Box, useInput } from './../../renderer.js';
import { Panel } from './panel/Panel.js';
import { Text } from './text/Text.js';
import { Icon, IconType } from './icon/Icon.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useTextStyle } from '../../hooks/useTextStyle.js';
import { Settings } from '../../constants/settings.js';
import { readBoolSetting } from '../../utils/cli-settings.js';
import {
  useAllowAsciiArt,
  useAllowAnimations,
  useAllowIcons,
  useShowThinking,
} from '../../hooks/useGlyphs.js';
import { useAppStore } from '../../stores/app-store.js';

interface ToggleItem {
  key: string;
  label: string;
  description: string;
  defaultValue: boolean;
  inverted?: boolean;
}

const ITEMS: ToggleItem[] = [
  {
    key: Settings.CHAT_ANIMATIONS,
    label: 'Animations',
    description:
      'When on: animated spinners (◑ ◐ ◑ ◐). When off: static indicators',
    defaultValue: true,
  },
  {
    key: Settings.CHAT_ASCII_MODE,
    label: 'ASCII art',
    description:
      'When on: braille dots, box-drawing, Unicode (⣿ ◑ ✓ │). When off: plain ASCII (+, |, -, /)',
    defaultValue: true,
  },
  {
    key: Settings.CHAT_ICONS,
    label: 'Icons',
    description:
      'When on: status icons (● running, ○ idle, ⚠ warning). When off: text-only labels',
    defaultValue: true,
  },
  {
    key: Settings.CHAT_SHOW_THINKING,
    label: 'Show thinking',
    description:
      'When on: display model reasoning/thinking content. When off: reasoning is hidden',
    defaultValue: true,
  },
];

interface DisplaySettingsPanelProps {
  onClose: () => void;
}

export const DisplaySettingsPanel: React.FC<DisplaySettingsPanelProps> = ({
  onClose,
}) => {
  const { getColor } = useTheme();
  const label = useTextStyle('label');
  const selectedLabel = useTextStyle('selectedLabel');
  const dimText = getColor('secondary');
  const brandText = getColor('primary');

  const [index, setIndex] = useState(0);
  const [values, setValues] = useState(() =>
    Object.fromEntries(
      ITEMS.map((item) => [
        item.key,
        readBoolSetting(item.key, item.defaultValue),
      ])
    )
  );
  const { setAllowAsciiArt } = useAllowAsciiArt();
  const { setAllowAnimations } = useAllowAnimations();
  const { setAllowIcons } = useAllowIcons();
  const { setShowThinking } = useShowThinking();
  const kiro = useAppStore((state) => state.kiro);

  const toggle = useCallback(
    (key: string) => {
      const newVal = !values[key];
      // Persist via ACP backend (locked read-modify-write)
      kiro.setSetting(key, newVal).catch(() => {});
      // Update local state
      setValues((prev) => ({ ...prev, [key]: newVal }));
      // Update context providers
      if (key === Settings.CHAT_ASCII_MODE) {
        setAllowAsciiArt(newVal);
      } else if (key === Settings.CHAT_ANIMATIONS) {
        setAllowAnimations(newVal);
      } else if (key === Settings.CHAT_ICONS) {
        setAllowIcons(newVal);
      } else if (key === Settings.CHAT_SHOW_THINKING) {
        setShowThinking(newVal);
      }
    },
    [
      values,
      kiro,
      setAllowAsciiArt,
      setAllowAnimations,
      setAllowIcons,
      setShowThinking,
    ]
  );

  useInput((input, key) => {
    if (key.upArrow) setIndex((i) => Math.max(0, i - 1));
    else if (key.downArrow) setIndex((i) => Math.min(ITEMS.length - 1, i + 1));
    else if (key.leftArrow || key.rightArrow || key.return)
      toggle(ITEMS[index]!.key);
  });

  return (
    <Panel
      title="/settings – display"
      onClose={onClose}
      footerLeft={<Text>{dimText('↑↓ select · ←→ toggle')}</Text>}
    >
      <Box flexDirection="column">
        <Box marginBottom={1}>
          <Text>{dimText('How would you like Kiro to display output?')}</Text>
        </Box>
        {ITEMS.map((item, i) => {
          const active = i === index;
          const rawVal = values[item.key];
          const displayVal = item.inverted ? !rawVal : rawVal;
          const val = displayVal ? 'on' : 'off';
          return (
            <Box key={item.key} flexDirection="row">
              {active ? (
                <Icon type={IconType.CHEVRON_RIGHT} color={selectedLabel} />
              ) : (
                <Text> </Text>
              )}
              <Text> </Text>
              <Text>
                {active ? selectedLabel(item.label) : label(item.label)}
              </Text>
              <Box width={2} />
              <Text>{brandText(val)}</Text>
              <Box width={2} />
              <Text>{dimText(item.description)}</Text>
            </Box>
          );
        })}
      </Box>
    </Panel>
  );
};
