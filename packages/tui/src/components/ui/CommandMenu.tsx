import React, { useCallback, useMemo, useState, useEffect } from 'react';
import { Menu } from '../ui/menu/Menu';
import { useAppStore } from '../../stores/app-store';
import { searchFiles } from '../../utils/file-search.js';

export const CommandMenu: React.FC = () => {
  const commandInputValue = useAppStore((state) => state.commandInputValue);
  const activeTrigger = useAppStore((state) => state.activeTrigger);
  const activeCommand = useAppStore((state) => state.activeCommand);
  const setActiveCommand = useAppStore((state) => state.setActiveCommand);
  const executeCommandWithArg = useAppStore((state) => state.executeCommandWithArg);
  const slashCommands = useAppStore((state) => state.slashCommands);
  const handleUserInput = useAppStore((state) => state.handleUserInput);
  const clearCommandInput = useAppStore((state) => state.clearCommandInput);
  const setCommandInput = useAppStore((state) => state.setCommandInput);
  const setPendingFileAttachment = useAppStore((state) => state.setPendingFileAttachment);
  const setFilePickerHasResults = useAppStore((state) => state.setFilePickerHasResults);
  const setActiveTrigger = useAppStore((state) => state.setActiveTrigger);

  // File search state
  const [fileResults, setFileResults] = useState<string[]>([]);

  // Extract @query from input
  const fileQuery = useMemo(() => {
    if (activeTrigger?.key !== '@') return '';
    const afterAt = commandInputValue.slice(activeTrigger.position + 1);
    const match = afterAt.match(/^(\S*)/);
    return match?.[1] ?? '';
  }, [commandInputValue, activeTrigger]);

  // Search files when query changes
  useEffect(() => {
    if (activeTrigger?.key === '@' && fileQuery) {
      const results = searchFiles(fileQuery);
      setFileResults(results);
      setFilePickerHasResults(results.length > 0);
    } else {
      setFileResults([]);
      setFilePickerHasResults(false);
    }
  }, [fileQuery, activeTrigger, setFilePickerHasResults]);

  const filteredCommands = useMemo(() => {
    if (activeTrigger?.key !== '/' || commandInputValue.includes(' ')) return [];
    const partial = commandInputValue.slice(1).toLowerCase();
    return slashCommands.filter(
      (cmd) => cmd.name.slice(1).toLowerCase().startsWith(partial)
    );
  }, [commandInputValue, slashCommands, activeTrigger]);

  const menuItems = useMemo(() => 
    filteredCommands.map((cmd) => ({
      label: cmd.name.slice(1),
      description: cmd.description,
    })),
    [filteredCommands]
  );

  const fileMenuItems = useMemo(() =>
    fileResults.map((path) => ({
      label: path,
      description: '',
    })),
    [fileResults]
  );

  const showCommandMenu = menuItems.length > 0 && activeTrigger?.key === '/';
  const showFilePicker = fileMenuItems.length > 0 && activeTrigger?.key === '@';

  const handleCommandSelect = useCallback(async (item: { label: string; description: string }) => {
    const fullCommand = `/${item.label}`;
    const cmd = slashCommands.find((c) => c.name === fullCommand);
    const isSelectionCommand = cmd?.meta?.inputType === 'selection';
    
    await handleUserInput(fullCommand);
    
    if (isSelectionCommand) {
      setCommandInput(fullCommand);
    }
  }, [handleUserInput, slashCommands, setCommandInput]);

  const handleFileSelect = useCallback((item: { label: string }) => {
    if (activeTrigger) {
      setPendingFileAttachment(item.label, activeTrigger.position);
      setActiveTrigger(null);
    }
  }, [activeTrigger, setPendingFileAttachment, setActiveTrigger]);

  const handleFilePickerEscape = useCallback(() => {
    setActiveTrigger(null);
  }, [setActiveTrigger]);

  if (showFilePicker && !activeCommand) {
    return (
      <Menu
        items={fileMenuItems}
        prefix="@"
        onSelect={handleFileSelect}
        onEscape={handleFilePickerEscape}
      />
    );
  }

  if (showCommandMenu && !activeCommand) {
    return (
      <Menu
        items={menuItems}
        prefix="/"
        onSelect={handleCommandSelect}
        onEscape={clearCommandInput}
      />
    );
  }

  if (activeCommand) {
    return (
      <Menu
        items={activeCommand.options.map((opt) => ({
          label: opt.label,
          description: opt.description ?? '',
        }))}
        prefix=""
        onSelect={(item) => {
          const opt = activeCommand.options.find((o) => o.label === item.label);
          if (opt) {
            clearCommandInput();
            executeCommandWithArg(opt.value);
          }
        }}
                onEscape={() => {
          setActiveCommand(null);
          clearCommandInput();
        }}
        showSelectedIndicator={true}
      />
    );
  }

  return null;
};
