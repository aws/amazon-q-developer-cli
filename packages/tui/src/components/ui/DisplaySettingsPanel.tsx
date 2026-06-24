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
  useThinkingMode,
  type ThinkingMode,
} from '../../hooks/useGlyphs.js';
import { useAppStore } from '../../stores/app-store.js';

interface ToggleItem {
  key: string;
  label: string;
  description: string;
  defaultValue: boolean;
  inverted?: boolean;
  /** When set, the item cycles through these string values instead of on/off. */
  cycle?: string[];
}

const THINKING_MODES: ThinkingMode[] = ['collapsed', 'expanded', 'off'];

const ITEMS: ToggleItem[] = [
  {
    key: Settings.CHAT_ANIMATIONS,
    label: 'Animations',
    description: 'Spinners, progress bars, and loading effects',
    defaultValue: true,
  },
  {
    key: Settings.CHAT_ASCII_MODE,
    label: 'ASCII art',
    description: 'Decorative text art including table lines',
    defaultValue: true,
  },
  {
    key: Settings.CHAT_ICONS,
    label: 'Icons',
    description: 'Symbols for status, actions, and labels',
    defaultValue: true,
  },
  {
    key: Settings.CHAT_SHOW_THINKING,
    label: 'Show thinking',
    description:
      'collapsed: header only (ctrl+o to view) · expanded: always show · off: hidden',
    defaultValue: true,
    cycle: THINKING_MODES,
  },
  {
    key: Settings.CHAT_TERMINAL_TITLE,
    label: 'Terminal title',
    description:
      'When on: update terminal window title with session info. When off: title unchanged',
    defaultValue: false,
  },
];

interface DisplaySettingsPanelProps {
  onClose: () => void;
  onDismiss?: () => void;
}

export const DisplaySettingsPanel: React.FC<DisplaySettingsPanelProps> = ({
  onClose,
  onDismiss,
}) => {
  const { getColor } = useTheme();
  const label = useTextStyle('label');
  const selectedLabel = useTextStyle('selectedLabel');
  const dimText = getColor('secondary');
  const brandText = getColor('primary');

  const { setAllowAsciiArt } = useAllowAsciiArt();
  const { setAllowAnimations } = useAllowAnimations();
  const { setAllowIcons } = useAllowIcons();
  const { thinkingMode, setThinkingMode } = useThinkingMode();
  const [index, setIndex] = useState(0);
  const [values, setValues] = useState<Record<string, boolean | string>>(() =>
    Object.fromEntries(
      ITEMS.map((item) => [
        item.key,
        item.cycle
          ? thinkingMode
          : readBoolSetting(item.key, item.defaultValue),
      ])
    )
  );
  const kiro = useAppStore((state) => state.kiro);
  const fromSettings = useAppStore((state) => state.settingsReturnOnEscape);
  const setTerminalTitleEnabled = useAppStore(
    (state) => state.setTerminalTitleEnabled
  );

  const toggle = useCallback(
    (key: string) => {
      const item = ITEMS.find((it) => it.key === key)!;
      if (item.cycle) {
        const cur = String(values[key]);
        const next =
          item.cycle[(item.cycle.indexOf(cur) + 1) % item.cycle.length]!;
        kiro.setSetting(key, next).catch(() => {});
        setValues((prev) => ({ ...prev, [key]: next }));
        if (key === Settings.CHAT_SHOW_THINKING) {
          setThinkingMode(next as ThinkingMode);
        }
        return;
      }
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
      } else if (key === Settings.CHAT_TERMINAL_TITLE) {
        setTerminalTitleEnabled(newVal);
      }
    },
    [
      values,
      kiro,
      setAllowAsciiArt,
      setAllowAnimations,
      setAllowIcons,
      setThinkingMode,
      setTerminalTitleEnabled,
    ]
  );

  useInput((input, key) => {
    if (key.upArrow) setIndex((i) => Math.max(0, i - 1));
    else if (key.downArrow) setIndex((i) => Math.min(ITEMS.length - 1, i + 1));
    else if (key.leftArrow || key.rightArrow) toggle(ITEMS[index]!.key);
    else if (key.return) {
      (onDismiss ?? onClose)();
    }
  });

  return (
    <Panel
      title="/settings – display"
      onClose={onClose}
      closeHintLabel={fromSettings ? 'to go back' : 'to close'}
      footerLeft={
        <Text>
          {brandText('↑↓')} {dimText('to select')}
          {dimText(' · ')}
          {brandText('←→')} {dimText('to toggle')}
          {dimText(' · ')}
          {brandText('↵')} {dimText('to apply and close')}
        </Text>
      }
    >
      <Box flexDirection="column">
        <Box height={1} />
        <Box marginBottom={1}>
          <Text>{dimText('How would you like Kiro to display output?')}</Text>
        </Box>
        {ITEMS.map((item, i) => {
          const active = i === index;
          const rawVal = values[item.key];
          const val = item.cycle
            ? String(rawVal)
            : (item.inverted ? !rawVal : rawVal)
              ? 'on'
              : 'off';
          return (
            <Box key={item.key} flexDirection="row">
              {active ? (
                <Icon type={IconType.CHEVRON_RIGHT} color={selectedLabel} />
              ) : (
                <Text> </Text>
              )}
              <Text> </Text>
              <Box width={16}>
                <Text>
                  {active ? selectedLabel(item.label) : label(item.label)}
                </Text>
              </Box>
              <Box width={11}>
                <Text>{brandText(val)}</Text>
              </Box>
              <Text>{dimText(item.description)}</Text>
            </Box>
          );
        })}
      </Box>
    </Panel>
  );
};
