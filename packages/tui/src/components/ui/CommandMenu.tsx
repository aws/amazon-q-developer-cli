import React, {
  useCallback,
  useMemo,
  useState,
  useEffect,
  useRef,
} from 'react';
import { Box } from './../../renderer.js';
import { Menu } from '../ui/menu/Menu';
import { Text } from '../ui/text/Text.js';
import { Divider } from '../ui/divider/Divider.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useAppStore } from '../../stores/app-store';
import { searchFilesAbortable } from '../../utils/file-search.js';
import {
  getBundledTheme,
  buildBundledPreview,
  getPromptPreset,
  getResponsePreset,
  promptPresets,
  responsePresets,
  loadUserThemePrefs,
} from '../../theme/user-theme.js';

export const CommandMenu: React.FC = () => {
  const { getColor } = useTheme();
  const secondaryColor = useMemo(() => getColor('secondary'), [getColor]);
  const commandInputValue = useAppStore((state) => state.commandInputValue);
  const activeTrigger = useAppStore((state) => state.activeTrigger);
  const activeCommand = useAppStore((state) => state.activeCommand);
  const setActiveCommand = useAppStore((state) => state.setActiveCommand);
  const executeCommandWithArg = useAppStore(
    (state) => state.executeCommandWithArg
  );
  const slashCommands = useAppStore((state) => state.slashCommands);
  const handleUserInput = useAppStore((state) => state.handleUserInput);
  const clearCommandInput = useAppStore((state) => state.clearCommandInput);
  const setCommandInput = useAppStore((state) => state.setCommandInput);
  const setPendingFileAttachment = useAppStore(
    (state) => state.setPendingFileAttachment
  );
  const setFilePickerHasResults = useAppStore(
    (state) => state.setFilePickerHasResults
  );
  const setActiveTrigger = useAppStore((state) => state.setActiveTrigger);
  const setPromptHint = useAppStore((state) => state.setPromptHint);
  const setThemePreview = useAppStore((state) => state.setThemePreview);

  // File search state
  const [fileResults, setFileResults] = useState<string[]>([]);

  // Add at the top of the component, after the existing useState/useCallback hooks:
  const highlightedRef = useRef<{ label: string; description: string } | null>(
    null
  );

  const handleCommandHighlight = useCallback(
    (item: { label: string; description: string }) => {
      highlightedRef.current = item;
    },
    []
  );

  const handleTabComplete = useCallback(() => {
    if (highlightedRef.current) {
      const fullCommand = `/${highlightedRef.current.label}`;
      const cmd = slashCommands.find((c) => c.name === fullCommand);
      const isPrompt = cmd?.meta?.type === 'prompt';

      // Fill the command into input with trailing space
      setCommandInput(`${fullCommand} `);

      // Show arg hints for prompts
      if (isPrompt && cmd?.meta?.arguments?.length) {
        setPromptHint(
          cmd.meta.arguments
            .map((a) => (a.required ? `<${a.name}>` : `[${a.name}]`))
            .join(' ')
        );
      } else {
        setPromptHint(null);
      }
    }
  }, [slashCommands, setCommandInput, setPromptHint]);

  // Extract @query from input
  const fileQuery = useMemo(() => {
    if (activeTrigger?.key !== '@') return '';
    const afterAt = commandInputValue.slice(activeTrigger.position + 1);
    const match = afterAt.match(/^(\S*)/);
    return match?.[1] ?? '';
  }, [commandInputValue, activeTrigger]);

  // Debounce file search. The search uses async opendir with AbortSignal,
  // so the previous walk is cancelled mid-flight when the query changes.
  useEffect(() => {
    if (activeTrigger?.key === '@' && fileQuery) {
      const ac = new AbortController();
      const timer = setTimeout(() => {
        searchFilesAbortable(fileQuery, ac.signal).then((results) => {
          if (ac.signal.aborted) return;
          setFileResults(results);
          setFilePickerHasResults(results.length > 0);
        });
      }, 100);
      return () => {
        clearTimeout(timer);
        ac.abort();
      };
    }
    setFileResults([]);
    setFilePickerHasResults(false);
  }, [fileQuery, activeTrigger, setFilePickerHasResults]);

  const filteredCommands = useMemo(() => {
    if (activeTrigger?.key !== '/' || commandInputValue.includes(' '))
      return [];
    const partial = commandInputValue.slice(1).toLowerCase();
    const matches = slashCommands.filter((cmd) =>
      cmd.name.slice(1).toLowerCase().startsWith(partial)
    );
    const cmds = matches.filter((c) => c.meta?.type !== 'prompt');
    const promptCmds = matches.filter((c) => c.meta?.type === 'prompt');
    cmds.sort((a, b) => a.name.localeCompare(b.name));
    return [...cmds, ...promptCmds];
  }, [commandInputValue, slashCommands, activeTrigger]);

  const menuItems = useMemo(
    () =>
      filteredCommands.map((cmd) => {
        const isPrompt = cmd.meta?.type === 'prompt';
        const argHints =
          isPrompt && cmd.meta?.arguments
            ? cmd.meta.arguments
                .map((arg) =>
                  arg.required ? `<${arg.name}>` : `[${arg.name}]`
                )
                .join(' ')
            : '';

        const typeLabel = isPrompt ? ' (prompt)' : '';

        return {
          label: cmd.name.slice(1),
          description: `${cmd.description}${typeLabel}${argHints ? ` ${argHints}` : ''}`,
        };
      }),
    [filteredCommands]
  );

  const fileMenuItems = useMemo(
    () =>
      fileResults.map((path) => ({
        label: path,
        description: '',
      })),
    [fileResults]
  );

  const showCommandMenu = menuItems.length > 0 && activeTrigger?.key === '/';
  const showFilePicker = fileMenuItems.length > 0 && activeTrigger?.key === '@';

  const handleCommandSelect = useCallback(
    async (item: { label: string; description: string }) => {
      const fullCommand = `/${item.label}`;
      const cmd = slashCommands.find((c) => c.name === fullCommand);
      const isPrompt = cmd?.meta?.type === 'prompt';

      // For prompts with args, prefill command and show arg hints
      if (isPrompt && cmd?.meta?.arguments?.length) {
        const argHint = cmd.meta.arguments
          .map((a) => (a.required ? `<${a.name}>` : `[${a.name}]`))
          .join(' ');
        setCommandInput(`${fullCommand} `);
        setPromptHint(argHint);
        return;
      }

      setPromptHint(null);
      await handleUserInput(fullCommand);
    },
    [handleUserInput, slashCommands, setCommandInput, setPromptHint]
  );

  const handleFileSelect = useCallback(
    (item: { label: string }) => {
      if (activeTrigger) {
        setPendingFileAttachment(item.label, activeTrigger.position);
        setActiveTrigger(null);
      }
    },
    [activeTrigger, setPendingFileAttachment, setActiveTrigger]
  );

  const handleFilePickerEscape = useCallback(() => {
    setActiveTrigger(null);
    setPromptHint(null);
  }, [setActiveTrigger, setPromptHint]);

  // Theme preview from store (set by effect handler during /theme flow)
  const themePreview = useAppStore((state) => state.themePreview);

  const isThemeMenu = activeCommand?.command.name === '/theme';

  // Clear theme preview when navigating away from /theme
  useEffect(() => {
    if (!isThemeMenu && themePreview) {
      setThemePreview(null);
    }
  }, [isThemeMenu, themePreview, setThemePreview]);

  const handleActiveCommandHighlight = useCallback(
    (item: { label: string; description: string }) => {
      if (!isThemeMenu || !activeCommand) return;
      const opt = activeCommand.options.find((o) => o.label === item.label);
      if (!opt) return;

      // Top-level: bundled theme preview
      if (opt.value.startsWith('bundled:')) {
        const themeId = opt.value.slice('bundled:'.length);
        const theme = getBundledTheme(themeId);
        if (theme) setThemePreview(buildBundledPreview(theme));
        return;
      }

      // Custom prompt/response preset: build preview combining highlighted preset with current other setting
      const prefs = loadUserThemePrefs();
      if (opt.value.startsWith('prompt:')) {
        const presetId = opt.value.slice('prompt:'.length);
        const prompt = getPromptPreset(presetId) ?? promptPresets[0]!;
        const response =
          getResponsePreset(prefs.responsePreset) ?? responsePresets[0]!;
        setThemePreview(
          buildBundledPreview({
            id: 'preview',
            label: 'Preview',
            prompt,
            response,
          })
        );
        return;
      }
      if (opt.value.startsWith('response:')) {
        const presetId = opt.value.slice('response:'.length);
        const prompt = getPromptPreset(prefs.promptPreset) ?? promptPresets[0]!;
        const response = getResponsePreset(presetId) ?? responsePresets[0]!;
        setThemePreview(
          buildBundledPreview({
            id: 'preview',
            label: 'Preview',
            prompt,
            response,
          })
        );
        return;
      }

      // Keep current preview for other options (Custom, Prompt style, Response text color)
    },
    [isThemeMenu, activeCommand, setThemePreview]
  );

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
        onHighlight={handleCommandHighlight}
        onTabComplete={handleTabComplete}
        onEscape={() => {
          clearCommandInput();
          setPromptHint(null);
        }}
      />
    );
  }

  if (activeCommand) {
    const isSelection = activeCommand.command.meta?.inputType === 'selection';
    // /feedback: selection menu without search (few options). /model, /agent: selection with search
    const isSearchable =
      isSelection && activeCommand.command.meta?.searchable !== false;
    return (
      <Box flexDirection="column">
        <Menu
          items={activeCommand.options.map((opt) => ({
            label: opt.label,
            description: opt.description ?? '',
            group: opt.group,
          }))}
          prefix=""
          onSelect={(item) => {
            const opt = activeCommand.options.find(
              (o) => o.label === item.label
            );
            if (opt) {
              if (opt.hint) {
                // Has required args — prefill command and show hint
                setCommandInput(`${opt.label} `);
                setPromptHint(opt.hint);
                setActiveCommand(null);
              } else {
                clearCommandInput();
                executeCommandWithArg(opt.value);
              }
            }
          }}
          onHighlight={handleActiveCommandHighlight}
          onEscape={() => {
            setActiveCommand(null);
            clearCommandInput();
            setPromptHint(null);
            setThemePreview(null);
          }}
          showSelectedIndicator={true}
          searchable={isSearchable}
          searchLabel={
            isSearchable
              ? `Select ${activeCommand.command.name.slice(1)}`
              : undefined
          }
          searchPlaceholder={isSearchable ? 'type to search' : undefined}
          showFooterHints={isSelection}
          preserveLabelColors={
            activeCommand.command.meta?.preserveLabelColors === true
          }
        />
        {themePreview && (
          <Box flexDirection="column" marginTop={1}>
            <Divider />
            <Box paddingX={1} flexDirection="column">
              <Text>{secondaryColor('Preview')}</Text>
              <Text>{themePreview}</Text>
            </Box>
          </Box>
        )}
      </Box>
    );
  }

  return null;
};
