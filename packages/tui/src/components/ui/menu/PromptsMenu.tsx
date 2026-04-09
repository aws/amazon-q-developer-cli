import React, { useState, useCallback } from 'react';
import { Box } from './../../../renderer.js';
import { Menu } from './Menu.js';
import { PromptDetails } from './PromptDetails.js';
import { findPromptByMenuLabel } from '../command-menu-utils.js';
import { useAppStore, type ActiveCommand } from '../../../stores/app-store.js';

/** Visible content rows for both the list and detail view. */
const VISIBLE_ITEMS = 8;
/** Chrome lines surrounding content: search/header + spacer + overflow + divider + footer. */
const CHROME_LINES = 5;
const MENU_HEIGHT = VISIBLE_ITEMS + CHROME_LINES;

export interface PromptsMenuProps {
  activeCommand: ActiveCommand;
  onDismiss: () => void;
}

export const PromptsMenu: React.FC<PromptsMenuProps> = ({
  activeCommand,
  onDismiss,
}) => {
  const slashCommands = useAppStore((s) => s.slashCommands);
  const executeCommandWithArg = useAppStore((s) => s.executeCommandWithArg);
  const clearCommandInput = useAppStore((s) => s.clearCommandInput);
  const setCommandInput = useAppStore((s) => s.setCommandInput);
  const setPromptHint = useAppStore((s) => s.setPromptHint);
  const setActiveCommand = useAppStore((s) => s.setActiveCommand);

  const [promptDetail, setPromptDetail] = useState<
    (typeof slashCommands)[number] | null
  >(null);

  const executeOption = useCallback(
    (label: string, fallbackValue?: string) => {
      const opt = activeCommand.options.find(
        (o) => o.label === label || o.label === label.replace(/^\//, '')
      );
      if (opt?.hint) {
        setCommandInput(`${opt.label} `);
        setPromptHint(opt.hint);
        setActiveCommand(null);
      } else {
        clearCommandInput();
        executeCommandWithArg(opt?.value ?? fallbackValue ?? label);
      }
    },
    [
      activeCommand.options,
      setCommandInput,
      setPromptHint,
      setActiveCommand,
      clearCommandInput,
      executeCommandWithArg,
    ]
  );

  const items = activeCommand.options.map((opt) => ({
    label: opt.label,
    description: opt.description ?? '',
    group: opt.group,
  }));

  return (
    <Box flexDirection="column" height={MENU_HEIGHT} overflow="hidden">
      {promptDetail ? (
        <PromptDetails
          key={promptDetail.name}
          name={promptDetail.name}
          description={promptDetail.description}
          meta={promptDetail.meta}
          visibleLines={VISIBLE_ITEMS}
          onBack={() => setPromptDetail(null)}
          onExecute={() => {
            setPromptDetail(null);
            executeOption(promptDetail.name, promptDetail.name);
          }}
        />
      ) : (
        <Menu
          items={items}
          prefix=""
          onSelect={(item) => executeOption(item.label)}
          onEscape={onDismiss}
          onRightArrow={(item) => {
            const cmd = findPromptByMenuLabel(slashCommands, item.label);
            if (cmd) setPromptDetail(cmd);
          }}
          showSelectedIndicator={true}
          visibleItems={VISIBLE_ITEMS}
          searchable={true}
          searchLabel="Select prompts"
          searchPlaceholder="type to search"
          showFooterHints={true}
        />
      )}
    </Box>
  );
};
