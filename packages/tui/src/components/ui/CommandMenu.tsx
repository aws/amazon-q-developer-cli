import React, {
  useCallback,
  useMemo,
  useState,
  useEffect,
  useRef,
} from 'react';
import { Box } from './../../renderer.js';
import { Menu } from '../ui/menu/Menu';
import { useAppStore } from '../../stores/app-store';
import { useCommandState } from '../../stores/selectors';
import type { AvailableCommand } from '../../types/commands';
import { searchFilesAbortable } from '../../utils/file-search.js';
import {
  filterPromptsByQuery,
  buildAtMenuItems,
} from './command-menu-utils.js';
import { PromptsMenu } from './menu/PromptsMenu.js';

export const CommandMenu: React.FC = () => {
  const commandInputValue = useAppStore((state) => state.commandInputValue);
  const activeTrigger = useAppStore((state) => state.activeTrigger);
  const activeCommand = useAppStore((state) => state.activeCommand);
  const setActiveCommand = useAppStore((state) => state.setActiveCommand);
  const executeCommandWithArg = useAppStore(
    (state) => state.executeCommandWithArg
  );
  const { slashCommands } = useCommandState();
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
  const settingsReturnOnEscape = useAppStore(
    (state) => state.settingsReturnOnEscape
  );
  const setSettingsReturnOnEscape = useAppStore(
    (state) => state.setSettingsReturnOnEscape
  );
  const reopenSettingsMenu = useAppStore((state) => state.reopenSettingsMenu);
  const setCommandShadowText = useAppStore(
    (state) => state.setCommandShadowText
  );
  const kiro = useAppStore((state) => state.kiro);

  // File search state
  const [fileResults, setFileResults] = useState<string[]>([]);

  // Track the currently highlighted item in the slash command dropdown
  const highlightedRef = useRef<{ label: string; description: string } | null>(
    null
  );

  const handleCommandHighlight = useCallback(
    (item: { label: string; description: string }) => {
      highlightedRef.current = item;
    },
    []
  );

  // Show the sub-command dropdown for a command that has sub-commands
  const showSubcommandMenu = useCallback(
    (cmd: AvailableCommand) => {
      const subs = cmd.meta?.subcommands;
      const subHints = cmd.meta?.subcommandHints ?? {};
      if (!subs || subs.length === 0) return false;

      const subOptions = subs.map((sub) => ({
        value: sub,
        label: sub,
        description: `${cmd.name} ${sub}`,
        hint: subHints[sub] ?? undefined,
      }));
      setActiveCommand({ command: cmd, options: subOptions });
      setCommandInput(`${cmd.name} `);
      setPromptHint(null);
      return true;
    },
    [setActiveCommand, setCommandInput, setPromptHint]
  );

  const handleTabComplete = useCallback(() => {
    if (highlightedRef.current) {
      const fullCommand = `/${highlightedRef.current.label}`;
      const cmd = slashCommands.find((c) => c.name === fullCommand);
      const isPrompt = cmd?.meta?.type === 'prompt';

      // If the command has sub-commands, show them in a dropdown
      if (cmd && showSubcommandMenu(cmd)) {
        return;
      }

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
  }, [slashCommands, setCommandInput, setPromptHint, showSubcommandMenu]);

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
    const cmds = matches.filter(
      (c) =>
        c.meta?.type !== 'prompt' &&
        c.meta?.type !== 'skill' &&
        c.meta?.type !== 'steering' &&
        !c.meta?.hidden
    );
    const promptCmds = matches.filter(
      (c) =>
        c.meta?.type === 'prompt' ||
        c.meta?.type === 'skill' ||
        c.meta?.type === 'steering'
    );
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
  // Also handles subcommand prefixes (e.g. /agent swap <name>)
  useEffect(() => {
    if (!commandInputValue.startsWith('/') || !commandInputValue.includes(' '))
      return;

    const spaceIdx = commandInputValue.indexOf(' ');
    const cmdName = commandInputValue.slice(0, spaceIdx);
    let partial = commandInputValue.slice(spaceIdx + 1);

    const cmd = slashCommands.find((c) => c.name === cmdName);
    if (!cmd || cmd.meta?.inputType !== 'selection' || !partial) {
      setCommandShadowText(null);
      return;
    }

    // Strip subcommand prefix (e.g. "swap ro" → "ro") so shadow text matches agent names
    const subs = cmd.meta?.subcommands;
    if (subs) {
      for (const sub of subs) {
        if (partial.startsWith(`${sub} `)) {
          partial = partial.slice(sub.length + 1);
          break;
        }
      }
    }

    if (!partial) {
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
        const subs = cmd.meta?.subcommands;
        const argHints =
          isPrompt && cmd.meta?.arguments
            ? cmd.meta.arguments
                .map((arg) =>
                  arg.required ? `<${arg.name}>` : `[${arg.name}]`
                )
                .join(' ')
            : '';

        const typeLabel = isPrompt ? ' (prompt)' : '';
        const subHint =
          subs && subs.length > 0 ? ' (tab for sub-commands)' : '';

        return {
          label: cmd.name.slice(1),
          description: `${cmd.description}${typeLabel}${argHints ? ` ${argHints}` : ''}${subHint}`,
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
    async (cmd: AvailableCommand) => {
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
        setPromptHint(null);
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
      setPromptHint,
    ]
  );

  const handleAtMenuEscape = useCallback(() => {
    setActiveTrigger(null);
    setPromptHint(null);
  }, [setActiveTrigger, setPromptHint]);

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
    const subs = activeCommand.command.meta?.subcommands;
    const isSubcommandMenu =
      subs &&
      subs.length > 0 &&
      activeCommand.options.length === subs.length &&
      activeCommand.options.every((o) => subs.includes(o.value));
    const isSearchable =
      !isSubcommandMenu &&
      isSelection &&
      activeCommand.command.meta?.searchable !== false;

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
              if (isSubcommandMenu) {
                // Sub-command selected: always prefill with the full command path.
                // If the sub-command needs args (has hint), show the hint.
                // If it doesn't need args, prefill and let the user press Enter to submit.
                const prefix = `${activeCommand.command.name} ${opt.label}`;
                setCommandInput(opt.hint ? `${prefix} ` : prefix);
                setPromptHint(opt.hint ?? null);
                setActiveCommand(null);
              } else if (opt.hint) {
                setCommandInput(`${opt.label} `);
                setPromptHint(opt.hint);
                setActiveCommand(null);
              } else {
                clearCommandInput();
                executeCommandWithArg(opt.value);
              }
            }
          }}
          onEscape={() => {
            // If this overlay was opened from /settings (e.g. the user is
            // now in the /theme menu reached via /settings → Theme), ESC
            // should return to the /settings top-level menu rather than
            // dismiss the whole overlay. The flag is set by the /settings
            // subcommand handlers and consumed (and cleared) here.
            const returnToSettings = settingsReturnOnEscape;

            // Only clear input if the command menu system owns it (slash
            // trigger active). When the subcommand dropdown was opened by
            // Tab from PromptInput, the user's text is in segments — not
            // commandInputValue — so clearing would wipe their prompt.
            setActiveCommand(null);
            if (activeTrigger) {
              clearCommandInput();
            }
            setPromptHint(null);

            if (returnToSettings) {
              setSettingsReturnOnEscape(false);
              // Re-open /settings directly. Going through handleUserInput
              // here caused the process to exit for reasons not fully
              // understood — likely races with the in-flight overlay close.
              reopenSettingsMenu();
            }
          }}
          showSelectedIndicator={true}
          searchable={isSearchable}
          searchLabel={
            isSubcommandMenu
              ? undefined
              : isSearchable
                ? `Select ${activeCommand.command.name.slice(1)}`
                : undefined
          }
          searchPlaceholder={isSearchable ? 'type to search' : undefined}
          showFooterHints={isSelection || isSubcommandMenu}
          preserveLabelColors={
            activeCommand.command.meta?.preserveLabelColors === true
          }
        />
      </Box>
    );
  }

  return null;
};
