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
} from '../../utils/cli-settings.js';
import { persistUiModeDefault } from '../../utils/ui-mode-default.js';
import type { UiMode } from '../../types/ui-mode.js';
import { ModeChangeSource } from '../../types/generated/chat-cli.js';
import {
  useGlyphs,
  useAllowAsciiArt,
  useAllowAnimations,
  useAllowIcons,
  useThinkingMode,
  type ThinkingMode,
} from '../../hooks/useGlyphs.js';
import { setVerboseConfig } from '../../lite/verbose.js';
import { useAppStore } from '../../stores/app-store.js';

interface ToggleItem {
  key: string;
  label: string;
  description: string;
  /** Absent on rows that open a panel instead of holding a value. */
  defaultValue?: boolean | string;
  inverted?: boolean;
  /** When set, the item cycles through these string values instead of on/off. */
  cycle?: string[];
  /** When set, the row opens a sub-panel instead of holding a value. */
  opensPanel?: 'statusLine';
}

const THINKING_MODES: ThinkingMode[] = ['collapsed', 'expanded', 'off'];

/** In-cohort, thinking display lives in /verbosity; off-cohort /verbosity is
 *  gated away, so restore the mainline "Show thinking" Display row there. */
const THINKING_ITEM: ToggleItem = {
  key: Settings.CHAT_SHOW_THINKING,
  label: 'Show thinking',
  description:
    'collapsed: header only (ctrl+o to view) · expanded: always show · off: hidden',
  defaultValue: true,
  cycle: THINKING_MODES,
};

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
    key: Settings.CHAT_SHOW_THINKING_TIPS,
    label: 'Thinking tips',
    description:
      'Show a feature tip below the thinking indicator while waiting',
    defaultValue: true,
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
 * Status-line row. Holds no value of its own: the segment list is long and
 * per-surface, so it lives in its own panel rather than as rows here.
 */
const STATUS_LINE_ITEM: ToggleItem = {
  key: 'statusLine',
  label: 'Status line',
  description: 'Choose which segments the status line shows',
  opensPanel: 'statusLine',
};

/**
 * The Display rows for the current rollout cohort. In-cohort: the full
 * ALL_ITEMS set (thinking lives in /verbosity). Off-cohort: drop the "Default
 * UI" (tui/lite) row — resolveUiMode() forces 'tui' there, so the toggle would
 * only persist a dead chat.ui.mode='lite' and emit telemetry — and restore the
 * "Show thinking" row (mainline had it here; /verbosity is gated away).
 */
export function selectDisplayItems(rolloutEnabled: boolean): ToggleItem[] {
  // Status line is appended once, after whichever set the cohort gets, so its
  // placement cannot drift between the two branches.
  const cohort = rolloutEnabled
    ? ALL_ITEMS
    : (() => {
        const items = ALL_ITEMS.filter(
          (item) => item.key !== Settings.CHAT_UI_MODE
        );
        // Restore the mainline row order: Show thinking sat before Terminal title.
        const titleIdx = items.findIndex(
          (i) => i.key === Settings.CHAT_TERMINAL_TITLE
        );
        const at = titleIdx === -1 ? items.length : titleIdx;
        return [...items.slice(0, at), THINKING_ITEM, ...items.slice(at)];
      })();
  return [...cohort, STATUS_LINE_ITEM];
}

interface DisplaySettingsPanelProps {
  surface: UiMode;
  onClose: () => void;
  onDismiss?: () => void;
  /** Hands off to the status-line panel, which replaces this one. */
  onOpenStatusLine?: () => void;
}

export const DisplaySettingsPanel: React.FC<DisplaySettingsPanelProps> = ({
  surface,
  onClose,
  onDismiss,
  onOpenStatusLine,
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
  const { thinkingMode } = useThinkingMode();

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
      ITEMS.filter((item) => !item.opensPanel).map((item) => [
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
  const setUiMode = useAppStore((state) => state.setUiMode);
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
          const nextUiMode = next as UiMode;
          if (surface !== nextUiMode) {
            setUiMode(nextUiMode);
            kiro.sendUiModeChanged({
              from: surface,
              to: nextUiMode,
              source: ModeChangeSource.SettingsPanel,
              sessionId: kiro.sessionId,
            });
          }
          persistUiModeDefault(nextUiMode, kiro);
        } else if (key === Settings.CHAT_SHOW_THINKING) {
          // Persist through the verbosity store so the version bump re-resolves
          // useGlyphs' thinkingMode live. This row is off-cohort only, where
          // resolveUiMode forces TUI.
          setVerboseConfig(
            {
              display: { thinkingDisplay: next as ThinkingMode },
            },
            'tui'
          );
        } else {
          kiro.setSetting(key, next).catch(() => {});
        }
        setValues((prev) => ({ ...prev, [key]: next }));
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
      ITEMS,
      kiro,
      surface,
      setUiMode,
      setAllowAsciiArt,
      setAllowAnimations,
      setAllowIcons,
      setTerminalTitleEnabled,
    ]
  );

  useInput((input, key) => {
    const item = ITEMS[index]!;
    if (key.upArrow) setIndex((i) => Math.max(0, i - 1));
    else if (key.downArrow) setIndex((i) => Math.min(ITEMS.length - 1, i + 1));
    else if (item.opensPanel) {
      // A navigating row holds no value, so both toggle keys and Enter open it.
      if (key.leftArrow || key.rightArrow || key.return) onOpenStatusLine?.();
    } else if (key.leftArrow || key.rightArrow) toggle(item.key);
    else if (key.return) {
      // Enter applies the highlighted row then closes; #2634 dropped the
      // toggle so Enter silently closed without switching the value.
      toggle(item.key);
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
          const val = item.opensPanel
            ? ''
            : item.cycle
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
