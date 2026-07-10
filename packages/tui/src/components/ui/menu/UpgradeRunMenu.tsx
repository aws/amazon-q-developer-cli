import React, { useState } from 'react';
import { Box } from './../../../renderer.js';
import { Text } from '../text/Text.js';
import { Divider } from '../divider/Divider.js';
import { Menu } from './Menu.js';
import { useTheme } from '../../../hooks/useThemeContext.js';
import { useAppStore } from '../../../stores/app-store.js';

export interface UpgradeRunMenuProps {
  onDismiss: () => void;
}

/**
 * `/upgrade-agent run` picker. Bucket-based selection (Enter upgrades the whole
 * highlighted bucket via the shared `executeCommandWithArg` dispatch, exactly
 * as the generic picker did). Below the menu, a read-only panel lists the agent
 * names in the highlighted bucket — the data is stashed in `upgradeRunPreview`
 * by the handler, keyed by the bucket's option value. Kept out of CommandMenu
 * so that router stays command-agnostic (mirrors UpgradeDiagnosticsMenu).
 */
export const UpgradeRunMenu: React.FC<UpgradeRunMenuProps> = ({
  onDismiss,
}) => {
  const { getColor } = useTheme();
  const dimText = getColor('secondary');

  const activeCommand = useAppStore((s) => s.activeCommand);
  const preview = useAppStore((s) => s.upgradeRunPreview);
  const executeCommandWithArg = useAppStore((s) => s.executeCommandWithArg);
  const clearCommandInput = useAppStore((s) => s.clearCommandInput);

  const options = activeCommand?.options ?? [];
  const [highlight, setHighlight] = useState<string | null>(null);

  const items = options.map((o) => ({
    label: o.label,
    description: o.description ?? '',
    group: o.group,
  }));

  const activeValue = highlight ?? options[0]?.value ?? '';
  const names = preview[activeValue] ?? [];

  return (
    <Box flexDirection="column">
      <Menu
        items={items}
        prefix=""
        onHighlight={(item) => {
          const opt = options.find((o) => o.label === item.label);
          setHighlight(opt?.value ?? null);
        }}
        onSelect={(item) => {
          const opt = options.find((o) => o.label === item.label);
          if (opt) {
            clearCommandInput();
            executeCommandWithArg(opt.value);
          }
        }}
        onEscape={onDismiss}
        showSelectedIndicator={true}
        searchable={true}
        searchLabel="Select upgrade-agent"
        searchPlaceholder="type to search"
        showFooterHints={true}
      />
      <Divider />
      <Box paddingX={1}>
        <Text>
          {dimText('Agents: ')}
          {names.length > 0 ? names.join(', ') : dimText('none')}
        </Text>
      </Box>
    </Box>
  );
};
