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
import { useGlyphs } from '../../hooks/useGlyphs.js';
import { useKeypress } from '../../hooks/useKeypress.js';
import { useAppStore } from '../../stores/app-store';
import { useCommandState } from '../../stores/selectors';
import type { AvailableCommand } from '../../types/commands';
import { searchFilesAbortable } from '../../utils/file-search.js';
import {
  filterPromptsByQuery,
  buildAtMenuItems,
  isCommandVisibleInUiMode,
} from './command-menu-utils.js';
import { PromptsMenu } from './menu/PromptsMenu.js';
import { UpgradeDiagnosticsMenu } from './menu/UpgradeDiagnosticsMenu.js';
import { UpgradeRunMenu } from './menu/UpgradeRunMenu.js';
import { VerbosityPreview } from './menu/VerbosityPreview.js';
import {
  VerbosityTruncationEditor,
  truncationConfigKey,
  type TruncationEditorField,
} from './menu/VerbosityTruncationEditor.js';
import { verbosityBreadcrumb } from './settings-panel-model.js';
import type { VerbosityPreviewKey } from '../../lite/render.js';
import {
  DENSITY_DISPLAY,
  DENSITY_FILTERS,
  DENSITY_PRESETS,
  type DensityPreset,
} from '../../lite/verbose.js';

// Ctrl+P is the master switch (hidden ↔ mini); `p` refines (mini → expanded).
// Two keys so a stray `p` while typing can't pop a preview.
type PreviewMode = 'mini' | 'expanded' | 'hidden';

// Runtime guard for the `store.previewKey: string` → VerbosityPreviewKey cast.
// Keep in sync if that union grows a fixture.
const VERBOSITY_PREVIEW_KEYS: readonly VerbosityPreviewKey[] = [
  'top',
  'density',
  'tool',
  'subagent',
  'output',
  'truncation:args',
  'truncation:output',
];

export const CommandMenu: React.FC = () => {
  const commandInputValue = useAppStore((state) => state.commandInputValue);
  const activeTrigger = useAppStore((state) => state.activeTrigger);
  const activeCommand = useAppStore((state) => state.activeCommand);
  const setActiveCommand = useAppStore((state) => state.setActiveCommand);
  const executeCommandWithArg = useAppStore(
    (state) => state.executeCommandWithArg
  );
  const { slashCommands: rawSlashCommands } = useCommandState();
  const uiMode = useAppStore((state) => state.uiMode);
  const slashCommands = useMemo(
    () => rawSlashCommands.filter((c) => isCommandVisibleInUiMode(c, uiMode)),
    [rawSlashCommands, uiMode]
  );
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
  const verboseReturnOnEscape = useAppStore(
    (state) => state.verboseReturnOnEscape
  );
  const setVerboseReturnOnEscape = useAppStore(
    (state) => state.setVerboseReturnOnEscape
  );
  const setCommandShadowText = useAppStore(
    (state) => state.setCommandShadowText
  );
  const kiro = useAppStore((state) => state.kiro);
  const { getColor } = useTheme();
  const glyphs = useGlyphs();
  const secondaryColor = useMemo(() => getColor('secondary'), [getColor]);

  const [fileResults, setFileResults] = useState<string[]>([]);

  const [previewMode, setPreviewMode] = useState<PreviewMode>('hidden');
  const [draftPreset, setDraftPreset] = useState<DensityPreset | null>(null);

  // Reset preview state ONLY when leaving /verbosity entirely — within it,
  // state must persist across submenu switches (density → tool → output) so
  // an armed preview doesn't disappear.
  const activeCommandName = activeCommand?.command.name ?? null;
  useEffect(() => {
    if (activeCommandName !== '/verbosity') {
      setPreviewMode('hidden');
      setDraftPreset(null);
    }
  }, [activeCommandName]);

  // Arming keymap. Gated to liteOnly commands with a preview fixture; Menu.tsx
  // yields Ctrl+P on liteOnly menus so this can claim it. (Ctrl+C-as-Esc and
  // the expanded-collapse live in the shared handler below.)
  useKeypress((input, key) => {
    const isLiteMenu =
      activeCommand?.command.meta?.liteOnly === true &&
      activeCommand.previewKey;
    if (!isLiteMenu) return;
    if (key.ctrl && (input === 'p' || input === 'P')) {
      setPreviewMode((m) => (m === 'hidden' ? 'mini' : 'hidden'));
      return;
    }
    if (
      previewMode !== 'expanded' &&
      previewMode !== 'hidden' &&
      (input === 'p' || input === 'P')
    ) {
      setPreviewMode('expanded');
    }
  });

  // Draft-render the highlighted /verbosity density preset in the preview;
  // non-preset rows clear it so the preview reverts to the saved config.
  const handleHighlight = (item: { label: string }) => {
    if (!activeCommand || activeCommand.command.name !== '/verbosity') return;
    const opt = activeCommand.options.find((o) => o.label === item.label);
    if (!opt) return;
    const m = opt.value.match(/^density:apply:([a-z]+)$/);
    setDraftPreset(
      m && DENSITY_PRESETS.includes(m[1] as DensityPreset)
        ? (m[1] as DensityPreset)
        : null
    );
  };

  // Remount the menu when its shape (command, option values, initialIndex)
  // changes so the cursor re-clamps; description-only changes (toggle re-opens
  // of the same submenu) keep the cursor on the toggled row.
  const activeCommandKey = activeCommand
    ? `${activeCommand.command.name}|${activeCommand.initialIndex ?? 0}|${activeCommand.options.map((o) => o.value).join('\0')}`
    : '';

  const highlightedRef = useRef<{ label: string; description: string } | null>(
    null
  );

  const handleCommandHighlight = useCallback(
    (item: { label: string; description: string }) => {
      highlightedRef.current = item;
    },
    []
  );

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

      if (cmd && showSubcommandMenu(cmd)) {
        return;
      }

      setCommandInput(`${fullCommand} `);

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

  const filteredPrompts = useMemo(
    () =>
      activeTrigger?.key === '@'
        ? filterPromptsByQuery(slashCommands, atQuery)
        : [],
    [activeTrigger, atQuery, slashCommands]
  );

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

  // Close the activeCommand overlay one level. Esc consumes the nested
  // return-on-escape stashes in priority order: verbose > settings, so
  // drilling in via /settings → Verbosity escapes one level at a time before
  // settings takes back over. Each stash is cleared on consume.
  const handleActiveCommandClose = useCallback(() => {
    const returnToSettings = settingsReturnOnEscape;
    const verboseReturn = verboseReturnOnEscape;

    // Only clear input if the command menu system owns it (slash trigger
    // active). When the subcommand dropdown was opened by Tab from
    // PromptInput, the user's text is in segments — not commandInputValue —
    // so clearing would wipe their prompt.
    setActiveCommand(null);
    if (activeTrigger) {
      clearCommandInput();
    }
    setPromptHint(null);

    // Verbose before settings (priority note above): step up ONE verbosity
    // level by re-dispatching the saved parent route; only once verboseReturn
    // is null does returnToSettings re-open /settings.
    if (verboseReturn) {
      setVerboseReturnOnEscape(null);
      handleUserInput(`/verbosity ${verboseReturn}`);
      return;
    }
    if (returnToSettings) {
      setSettingsReturnOnEscape(false);
      // Re-open directly: routing through handleUserInput here exited the
      // process (footgun — likely a race with the in-flight overlay close).
      reopenSettingsMenu();
      return;
    }
  }, [
    activeTrigger,
    settingsReturnOnEscape,
    verboseReturnOnEscape,
    setActiveCommand,
    clearCommandInput,
    setPromptHint,
    setSettingsReturnOnEscape,
    setVerboseReturnOnEscape,
    reopenSettingsMenu,
    handleUserInput,
  ]);

  // Stashed = drilled in from a parent menu, so Esc steps back, not closes.
  const hasReturnStash = settingsReturnOnEscape || verboseReturnOnEscape;

  useKeypress((input, key) => {
    // Ctrl+C inside any menu surface = Esc (one level). Without this it falls
    // through to AppContainer's double-Ctrl+C quit flow — a startling
    // overreaction to backing out of a menu.
    if (!(key.ctrl && input === 'c')) return;
    if (previewMode === 'expanded') {
      // Collapse to the menu, matching the pane's own Esc; closing here would
      // dump the whole /verbosity menu.
      setPreviewMode('mini');
      return;
    }
    if (activeCommand) {
      if (activeCommand.command.name === '/prompts') {
        setActiveCommand(null);
        clearCommandInput();
        setPromptHint(null);
        return;
      }
      handleActiveCommandClose();
      return;
    }
    if (showCommandMenu) {
      clearCommandInput();
      setPromptHint(null);
      return;
    }
    if (showAtMenu) {
      handleAtMenuEscape();
    }
  });

  if (showAtMenu && !activeCommand) {
    return (
      <Menu
        items={atMenuItems}
        prefix="@"
        onSelect={handleAtMenuSelect}
        onEscape={handleAtMenuEscape}
        showFooterHints={true}
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
        showFooterHints={true}
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

    if (activeCommand.previewKey === 'upgrade-diagnostics') {
      return (
        <UpgradeDiagnosticsMenu
          onDismiss={() => {
            setActiveCommand(null);
            clearCommandInput();
            setPromptHint(null);
          }}
        />
      );
    }

    if (activeCommand.previewKey === 'upgrade-run') {
      return (
        <UpgradeRunMenu
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

    // Truncation editor mode: previewKey ends in `:edit`. Swap the menu for the
    // numeric editor, which routes back via executeCommandWithArg on commit.
    const previewKey = activeCommand.previewKey;
    const truncEditMatch =
      previewKey &&
      previewKey.match(
        /^truncation:(argsLines|argsChars|outputLines|outputChars):edit$/
      );

    // Lite /verbosity gets the settings panel chrome: a breadcrumb header +
    // divider where the input row sat (LiteLayout hides it; see
    // isLiteVerbosityMenu there). Gated to lite — TUI filters /verbosity out.
    const isLiteVerbosityMenu =
      uiMode === 'lite' && activeCommand.command.name === '/verbosity';
    const verbosityHeader = isLiteVerbosityMenu ? (
      <Box flexDirection="column">
        <Box paddingX={1}>
          <Text>{getColor('primary')(verbosityBreadcrumb(previewKey))}</Text>
        </Box>
        <Divider />
      </Box>
    ) : null;

    if (truncEditMatch) {
      const which = truncEditMatch[1] as TruncationEditorField;
      const settingKey = truncationConfigKey(which);
      return (
        <Box flexDirection="column">
          {verbosityHeader}
          <VerbosityTruncationEditor
            which={which}
            onCommit={(value) => {
              clearCommandInput();
              executeCommandWithArg(
                `set:${settingKey}:${value === null ? 'null' : value}`
              );
            }}
            onCancel={handleActiveCommandClose}
          />
        </Box>
      );
    }

    // 'truncation' is a submenu, not a fixture key; map it to 'top' so a
    // generic mix shows next to the cap rows.
    const verbosityPreviewKey: VerbosityPreviewKey | null =
      previewKey === 'truncation'
        ? 'top'
        : (VERBOSITY_PREVIEW_KEYS.find((k) => k === previewKey) ?? null);

    // A highlighted density preset draft-renders that preset's display/filters
    // in the preview without persisting; no draft = saved config.
    const draftDisplay = draftPreset ? DENSITY_DISPLAY[draftPreset] : undefined;
    const draftFilters = draftPreset ? DENSITY_FILTERS[draftPreset] : undefined;

    // Expanded preview: swap the menu for the scrollable pane (owns its keys).
    if (previewMode === 'expanded' && verbosityPreviewKey) {
      return (
        <Box flexDirection="column">
          {verbosityHeader}
          <VerbosityPreview
            mode="expanded"
            which={verbosityPreviewKey}
            displayOverride={draftDisplay}
            filtersOverride={draftFilters}
            onCollapse={() => setPreviewMode('mini')}
            onHide={() => setPreviewMode('hidden')}
          />
        </Box>
      );
    }

    return (
      <Box flexDirection="column">
        {verbosityHeader}
        <Menu
          key={activeCommandKey}
          initialIndex={activeCommand.initialIndex}
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
                // Prefill the full path; trailing space only when the
                // sub-command takes args (has a hint).
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
          onHighlight={handleHighlight}
          onEscape={handleActiveCommandClose}
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
          liteOnly={activeCommand.command.meta?.liteOnly === true}
          closeMenuActionLabel={
            hasReturnStash ? `${glyphs.arrowLeft} back` : 'to close'
          }
        />
        {verbosityPreviewKey && previewMode === 'mini' && (
          <VerbosityPreview
            which={verbosityPreviewKey}
            displayOverride={draftDisplay}
            filtersOverride={draftFilters}
          />
        )}
        {verbosityPreviewKey && (
          <Box paddingX={1}>
            <Text>
              {secondaryColor(
                previewMode === 'hidden'
                  ? '  ctrl+p to show preview'
                  : `  p to expand ${glyphs.smallDot} ctrl+p to hide preview`
              )}
            </Text>
          </Box>
        )}
      </Box>
    );
  }

  return null;
};
