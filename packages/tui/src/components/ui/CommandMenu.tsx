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
import { useAppStore, type SlashCommand } from '../../stores/app-store';
import { searchFilesAbortable } from '../../utils/file-search.js';
import {
  getBundledTheme,
  buildBundledPreview,
  buildCurrentPreview,
  buildFallbackDiff,
  getPromptPreset,
  getResponsePreset,
  getDiffPreset,
  promptPresets,
  responsePresets,
  diffPresets,
  loadUserThemePrefs,
} from '../../theme/user-theme.js';
import {
  filterPromptsByQuery,
  buildAtMenuItems,
} from './command-menu-utils.js';
import { PromptsMenu } from './menu/PromptsMenu.js';

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
  const setCommandShadowText = useAppStore(
    (state) => state.setCommandShadowText
  );
  const kiro = useAppStore((state) => state.kiro);
  const setThemePreview = useAppStore((state) => state.setThemePreview);
  const getAutoPreview = useAppStore((state) => state._autoPreviewGetter);

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
  const atQuery = useMemo(() => {
    if (activeTrigger?.key !== '@') return '';
    const afterAt = commandInputValue.slice(activeTrigger.position + 1);
    const match = afterAt.match(/^(\S*)/);
    return match?.[1] ?? '';
  }, [commandInputValue, activeTrigger]);

  // Debounce file search. The search uses async opendir with AbortSignal,
  // so the previous walk is cancelled mid-flight when the query changes.
  useEffect(() => {
    if (activeTrigger?.key === '@' && atQuery) {
      const ac = new AbortController();
      const timer = setTimeout(() => {
        searchFilesAbortable(atQuery, ac.signal).then((results) => {
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
  }, [atQuery, activeTrigger, setFilePickerHasResults]);

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

  // No shadow text for top-level command menu — the dropdown handles that.
  // Shadow text is only for argument completion (e.g. /model clau → de-opus-4.6).

  // Cache options per command to avoid re-fetching on every keystroke.
  const optionsCacheRef = useRef<{
    cmdName: string;
    options: Array<{ label: string; value: string }>;
  }>({ cmdName: '', options: [] });

  // Argument shadow text for selection commands (e.g. /agent <name>, /model <name>)
  useEffect(() => {
    if (!commandInputValue.startsWith('/') || !commandInputValue.includes(' '))
      return;

    const spaceIdx = commandInputValue.indexOf(' ');
    const cmdName = commandInputValue.slice(0, spaceIdx);
    const partial = commandInputValue.slice(spaceIdx + 1);

    const cmd = slashCommands.find((c) => c.name === cmdName);
    if (!cmd || cmd.meta?.inputType !== 'selection' || !partial) {
      setCommandShadowText(null);
      return;
    }

    // Use cached options for instant matching (no blink)
    const cache = optionsCacheRef.current;
    if (cache.cmdName === cmdName && cache.options.length > 0) {
      const match = cache.options.find((o) =>
        o.label.toLowerCase().startsWith(partial.toLowerCase())
      );
      setCommandShadowText(
        match && match.label.length > partial.length
          ? match.label.slice(partial.length)
          : null
      );
    }

    // Fetch (or refresh) options in background
    let cancelled = false;
    const needsFetch = cache.cmdName !== cmdName;
    if (needsFetch && kiro?.getCommandOptions) {
      const timer = setTimeout(async () => {
        try {
          const result = await kiro.getCommandOptions(cmd.name, '');
          if (cancelled) return;
          const options = (result?.options ?? []) as Array<{
            label: string;
            value: string;
          }>;
          optionsCacheRef.current = { cmdName, options };
          const match = options.find((o) =>
            o.label.toLowerCase().startsWith(partial.toLowerCase())
          );
          setCommandShadowText(
            match && match.label.length > partial.length
              ? match.label.slice(partial.length)
              : null
          );
        } catch {
          // Silently ignore
        }
      }, 50);
      return () => {
        cancelled = true;
        clearTimeout(timer);
      };
    }
  }, [commandInputValue, slashCommands, kiro, setCommandShadowText]);

  // Clean up shadow text on unmount
  useEffect(() => () => setCommandShadowText(null), [setCommandShadowText]);

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

  // Filter prompts matching @query
  const filteredPrompts = useMemo(
    () =>
      activeTrigger?.key === '@'
        ? filterPromptsByQuery(slashCommands, atQuery)
        : [],
    [activeTrigger, atQuery, slashCommands]
  );

  // Unified @ menu: prompts first, then files
  const atMenuItems = useMemo(
    () => buildAtMenuItems(filteredPrompts, fileResults),
    [filteredPrompts, fileResults]
  );

  const showAtMenu = atMenuItems.length > 0 && activeTrigger?.key === '@';

  const showCommandMenu = menuItems.length > 0 && activeTrigger?.key === '/';

  // Shared: prefill args or execute a prompt command
  const executePromptOrPrefill = useCallback(
    async (cmd: SlashCommand) => {
      if (cmd.meta?.arguments?.length) {
        const argHint = cmd.meta.arguments
          .map((a) => (a.required ? `<${a.name}>` : `[${a.name}]`))
          .join(' ');
        setCommandInput(`${cmd.name} `);
        setPromptHint(argHint);
        setActiveTrigger(null);
        return;
      }
      await handleUserInput(cmd.name);
    },
    [handleUserInput, setCommandInput, setPromptHint, setActiveTrigger]
  );

  const handleCommandSelect = useCallback(
    async (item: { label: string; description: string }) => {
      const cmd = slashCommands.find((c) => c.name === `/${item.label}`);
      if (cmd?.meta?.type === 'prompt') {
        await executePromptOrPrefill(cmd);
        return;
      }
      setPromptHint(null);
      await handleUserInput(`/${item.label}`);
    },
    [slashCommands, handleUserInput, setPromptHint, executePromptOrPrefill]
  );

  const handleAtMenuSelect = useCallback(
    async (item: { label: string; description: string; group?: string }) => {
      if (item.group === 'Prompt') {
        const cmd = slashCommands.find((c) => c.name === `/${item.label}`);
        if (cmd) await executePromptOrPrefill(cmd);
        return;
      }
      if (activeTrigger) {
        setPendingFileAttachment(item.label, activeTrigger.position);
        setActiveTrigger(null);
      }
    },
    [
      slashCommands,
      activeTrigger,
      executePromptOrPrefill,
      setPendingFileAttachment,
      setActiveTrigger,
    ]
  );

  const handleAtMenuEscape = useCallback(() => {
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

      const fallbackDiff = buildFallbackDiff({
        added: {
          background: getColor('diff.added.background').hex,
          bar: getColor('diff.added.bar').hex,
          highlight: getColor('diff.added.highlight').hex,
        },
        removed: {
          background: getColor('diff.removed.background').hex,
          bar: getColor('diff.removed.bar').hex,
          highlight: getColor('diff.removed.highlight').hex,
        },
      });

      // Top-level: bundled theme preview
      if (opt.value.startsWith('bundled:')) {
        const themeId = opt.value.slice('bundled:'.length);
        if (themeId === 'default') {
          // Auto — show base theme preview with no user overrides
          const preview = getAutoPreview?.();
          setThemePreview(preview || null);
          return;
        }
        const theme = getBundledTheme(themeId);
        if (theme) setThemePreview(buildBundledPreview(theme, fallbackDiff));
        return;
      }

      // Custom option — show current prefs preview
      if (opt.value === 'custom') {
        const currentPrefs = loadUserThemePrefs();
        setThemePreview(buildCurrentPreview(currentPrefs, fallbackDiff));
        return;
      }

      // Custom prompt/response preset: build preview combining highlighted preset with current other setting
      const prefs = loadUserThemePrefs();
      if (opt.value.startsWith('prompt:')) {
        const presetId = opt.value.slice('prompt:'.length);
        const prompt = getPromptPreset(presetId) ?? promptPresets[0]!;
        const response =
          getResponsePreset(prefs.responsePreset) ?? responsePresets[0]!;
        const diff = getDiffPreset(prefs.diffPreset) ?? diffPresets[0]!;
        setThemePreview(
          buildBundledPreview(
            {
              id: 'preview',
              label: 'Preview',
              prompt,
              response,
              diff,
            },
            fallbackDiff
          )
        );
        return;
      }
      if (opt.value.startsWith('response:')) {
        const presetId = opt.value.slice('response:'.length);
        const prompt = getPromptPreset(prefs.promptPreset) ?? promptPresets[0]!;
        const response = getResponsePreset(presetId) ?? responsePresets[0]!;
        const diff = getDiffPreset(prefs.diffPreset) ?? diffPresets[0]!;
        setThemePreview(
          buildBundledPreview(
            {
              id: 'preview',
              label: 'Preview',
              prompt,
              response,
              diff,
            },
            fallbackDiff
          )
        );
        return;
      }
      if (opt.value.startsWith('diff:')) {
        const presetId = opt.value.slice('diff:'.length);
        const prompt = getPromptPreset(prefs.promptPreset) ?? promptPresets[0]!;
        const response =
          getResponsePreset(prefs.responsePreset) ?? responsePresets[0]!;
        const diff = getDiffPreset(presetId) ?? diffPresets[0]!;
        setThemePreview(
          buildBundledPreview(
            {
              id: 'preview',
              label: 'Preview',
              prompt,
              response,
              diff,
            },
            fallbackDiff
          )
        );
        return;
      }

      // Keep current preview for other options (Custom, Prompt style, Response text color)
    },
    [isThemeMenu, activeCommand, setThemePreview, getColor]
  );

  if (showAtMenu && !activeCommand) {
    return (
      <Menu
        items={atMenuItems}
        prefix="@"
        onSelect={handleAtMenuSelect}
        onEscape={handleAtMenuEscape}
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
    if (activeCommand.command.name === '/prompts') {
      return (
        <PromptsMenu
          activeCommand={activeCommand}
          onDismiss={() => {
            setActiveCommand(null);
            clearCommandInput();
            setPromptHint(null);
          }}
        />
      );
    }

    const isSelection = activeCommand.command.meta?.inputType === 'selection';
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
