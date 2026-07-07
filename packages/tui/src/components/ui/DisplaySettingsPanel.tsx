import React, { useState, useCallback, useMemo } from 'react';
import { Box, useInput } from './../../renderer.js';
import { Panel } from './panel/Panel.js';
import { Text } from './text/Text.js';
import { Icon, IconType } from './icon/Icon.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useTextStyle } from '../../hooks/useTextStyle.js';
import { Settings } from '../../constants/settings.js';
import {
  readBoolSetting,
  readStringSetting,
  readCliSettings,
  writeCliSettings,
} from '../../utils/cli-settings.js';
import {
  useGlyphs,
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
  defaultValue: boolean | string;
  inverted?: boolean;
  /** When set, the item cycles through these string values instead of on/off. */
  cycle?: string[];
}

const THINKING_MODES: ThinkingMode[] = ['collapsed', 'expanded', 'off'];

/** Normalize the persisted chat.ui.mode setting for telemetry payloads. */
function normalizeUiModeForTelemetry(raw: string): 'lite' | 'tui' | 'unset' {
  return raw === 'lite' || raw === 'tui' ? raw : 'unset';
}

const ALL_ITEMS: ToggleItem[] = [
  {
    key: Settings.CHAT_UI_MODE,
    label: 'Default UI',
    description: 'UI launched when you open Kiro CLI (lite or tui)',
    defaultValue: 'tui',
    cycle: ['tui', 'lite'],
  },
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

/**
 * The Display rows for the current rollout cohort. The "Default UI" (tui/lite)
 * row is dropped outside the cohort (KIRO_LITE_ROLLOUT_ENABLED !== '1'): the
 * same gate resolveUiMode() and switchToLite() read. Outside the cohort
 * resolveUiMode forces 'tui', so the toggle would only persist a dead
 * chat.ui.mode='lite' value and emit uiModeDefaultChanged telemetry — a
 * leaking affordance with no effect. The other rows are legitimately
 * cross-mode, so we gate the row, not the panel.
 */
export function selectDisplayItems(rolloutEnabled: boolean): ToggleItem[] {
  return rolloutEnabled
    ? ALL_ITEMS
    : ALL_ITEMS.filter((item) => item.key !== Settings.CHAT_UI_MODE);
}

interface DisplaySettingsPanelProps {
  onClose: () => void;
  onDismiss?: () => void;
}

export const DisplaySettingsPanel: React.FC<DisplaySettingsPanelProps> = ({
  onClose,
  onDismiss,
}) => {
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const label = useTextStyle('label');
  const selectedLabel = useTextStyle('selectedLabel');
  const dimText = getColor('secondary');
  const brandText = getColor('primary');

  const { setAllowAsciiArt } = useAllowAsciiArt();
  const { setAllowAnimations } = useAllowAnimations();
  const { setAllowIcons } = useAllowIcons();
  const { thinkingMode, setThinkingMode } = useThinkingMode();

  // Route separators embedded in item descriptions through glyphs so ASCII
  // mode renders '.' instead of '·' (uniform with the rest of the panel).
  const ITEMS = useMemo(
    () =>
      selectDisplayItems(process.env.KIRO_LITE_ROLLOUT_ENABLED === '1').map(
        (item) => ({
          ...item,
          description: item.description.split('·').join(glyphs.smallDot),
        })
      ),
    [glyphs]
  );

  const [index, setIndex] = useState(0);
  const [values, setValues] = useState<Record<string, boolean | string>>(() =>
    Object.fromEntries(
      ITEMS.map((item) => [
        item.key,
        item.cycle
          ? item.key === Settings.CHAT_SHOW_THINKING
            ? thinkingMode
            : readStringSetting(item.key, String(item.defaultValue))
          : readBoolSetting(item.key, item.defaultValue === true),
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
        if (key === Settings.CHAT_UI_MODE) {
          // Default UI: dual-write (cli.json + ACP setSetting) so the next
          // session boots into the chosen layout, plus emit the
          // uiModeDefaultChanged telemetry event when the value actually
          // changes. Mirrors the dispatch the old `/settings default-ui:<mode>`
          // handler used to do — only the entry point moved into this panel.
          const previous = normalizeUiModeForTelemetry(cur);
          const settings = readCliSettings();
          settings[key] = next;
          writeCliSettings(settings);
          kiro.setSetting(key, next).catch(() => {});
          if (previous !== next) {
            kiro.sendUiModeDefaultChanged?.({
              from: previous,
              to: next as 'lite' | 'tui',
              sessionId: kiro.sessionId,
            });
          }
        } else {
          kiro.setSetting(key, next).catch(() => {});
        }
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
      // Enter applies the highlighted row then closes; #2634 dropped the
      // toggle so Enter silently closed without switching the value.
      toggle(ITEMS[index]!.key);
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
          {brandText(`${glyphs.arrowUp}${glyphs.arrowDown}`)}{' '}
          {dimText('to select')}
          {dimText(` ${glyphs.smallDot} `)}
          {brandText(`${glyphs.arrowLeft}${glyphs.arrow}`)}{' '}
          {dimText('to toggle')}
          {dimText(` ${glyphs.smallDot} `)}
          {brandText(glyphs.enter)} {dimText('to apply and close')}
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
