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
import { useKeypress } from '../../hooks/useKeypress.js';
import { useAppStore } from '../../stores/app-store';
import { useCommandState } from '../../stores/selectors';
import type { AvailableCommand } from '../../types/commands';
import { searchFilesAbortable } from '../../utils/file-search.js';
import {
  filterPromptsByQuery,
  buildAtMenuItems,
} from './command-menu-utils.js';
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
import { PromptsMenu } from './menu/PromptsMenu.js';
import { VerbosityPreview } from './menu/VerbosityPreview.js';
import { VerbosityPreviewPane } from './menu/VerbosityPreviewPane.js';
import { VerbosityTruncationEditor } from './menu/VerbosityTruncationEditor.js';
import { verbosityBreadcrumb } from './settings-panel-model.js';
import type { VerbosityPreviewKey } from '../../lite/render.js';
import {
  DENSITY_DISPLAY,
  DENSITY_FILTERS,
  DENSITY_PRESETS,
  type DensityPreset,
} from '../../lite/verbose.js';

const VERBOSITY_PREVIEW_KEYS = new Set<string>([
  'top',
  'density',
  'tool',
  'subagent',
  'output',
  'truncation:args',
  'truncation:output',
]);

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
  // Lite-only commands (e.g. /verbose) shouldn't surface in TUI mode — they
  // bind to lite-mode rendering hooks and would no-op or error there.
  const slashCommands = useMemo(
    () =>
      uiMode === 'lite'
        ? rawSlashCommands
        : rawSlashCommands.filter((c) => c.meta?.liteOnly !== true),
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
  const themeReturnOnEscape = useAppStore((state) => state.themeReturnOnEscape);
  const setThemeReturnOnEscape = useAppStore(
    (state) => state.setThemeReturnOnEscape
  );
  const setCommandShadowText = useAppStore(
    (state) => state.setCommandShadowText
  );
  const kiro = useAppStore((state) => state.kiro);
  const { getColor, colors: themeColors } = useTheme();
  const secondaryColor = useMemo(() => getColor('secondary'), [getColor]);
  const setThemePreview = useAppStore((state) => state.setThemePreview);
  const getAutoPreview = useAppStore((state) => state._autoPreviewGetter);

  // File search state
  const [fileResults, setFileResults] = useState<string[]>([]);

  // /verbosity preview. Two keys so a stray `p` while typing can't pop a
  // preview: Ctrl+P is the master switch (hidden ↔ mini), `p` is the refine
  // toggle (mini ↔ expanded).
  type PreviewMode = 'mini' | 'expanded' | 'hidden';
  const [previewMode, setPreviewMode] = useState<PreviewMode>('hidden');

  // Highlighted density preset, draft-previewed before commit; null when the
  // cursor isn't on a preset row.
  const [draftPreset, setDraftPreset] = useState<DensityPreset | null>(null);

  // Menu key: remount when the menu's shape (command, option values) or
  // initialIndex changes so the cursor re-clamps and initialIndex re-applies.
  // Description-only changes are ignored (toggle re-opens of the same submenu
  // produce identical option values) so the cursor stays on the toggled row.
  const activeCommandKey = activeCommand
    ? `${activeCommand.command.name}|${activeCommand.initialIndex ?? 0}|${activeCommand.options.map((o) => o.value).join('\0')}`
    : '';
  // Reset preview state ONLY when leaving /verbosity entirely. Within it,
  // preview state persists across submenu switches so an armed preview
  // doesn't disappear when walking density → tool → output. draftPreset is
  // also nulled here so a stale preset can't leak into a future session.
  const activeCommandName = activeCommand?.command.name ?? null;
  useEffect(() => {
    if (activeCommandName !== '/verbosity') {
      if (previewMode !== 'hidden') setPreviewMode('hidden');
      if (draftPreset !== null) setDraftPreset(null);
    }
    // Depend only on the command name: submenu shape changes within
    // /verbosity must not trigger a reset, and previewMode/draftPreset are
    // read-only here (we act on the command-leaving transition only).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeCommandName]);

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

  // Close the activeCommand overlay one level. Esc consumes the nested
  // return-on-escape stashes in priority order: theme > verbose > settings,
  // so drilling in via /settings → Theme escapes one /theme level at a time
  // before settings takes back over. Each stash is cleared on consume.
  const handleActiveCommandClose = useCallback(() => {
    const returnToSettings = settingsReturnOnEscape;
    const verboseReturn = verboseReturnOnEscape;
    // Empty string = "go back to bare /theme"; null = "close the overlay".
    const themeReturn = themeReturnOnEscape;

    // Only clear input if the command menu system owns it (slash trigger
    // active). When the subcommand dropdown was opened by Tab from
    // PromptInput, the user's text is in segments — not commandInputValue —
    // so clearing would wipe their prompt.
    setActiveCommand(null);
    if (activeTrigger) {
      clearCommandInput();
    }
    setPromptHint(null);
    setThemePreview(null);

    if (themeReturn !== null) {
      setThemeReturnOnEscape(null);
      handleUserInput(themeReturn === '' ? '/theme' : `/theme ${themeReturn}`);
      return;
    }
    // Verbose before settings (see priority note above): step up ONE
    // verbosity level by re-dispatching with the saved parent route. Only
    // once verboseReturn is null does returnToSettings re-open /settings.
    if (verboseReturn) {
      setVerboseReturnOnEscape(null);
      handleUserInput(`/verbosity ${verboseReturn}`);
      return;
    }
    if (returnToSettings) {
      setSettingsReturnOnEscape(false);
      // Re-open directly: routing through handleUserInput here exited the
      // process — likely a race with the in-flight overlay close.
      reopenSettingsMenu();
      return;
    }
  }, [
    activeTrigger,
    settingsReturnOnEscape,
    verboseReturnOnEscape,
    themeReturnOnEscape,
    setActiveCommand,
    clearCommandInput,
    setPromptHint,
    setSettingsReturnOnEscape,
    setVerboseReturnOnEscape,
    setThemeReturnOnEscape,
    setThemePreview,
    reopenSettingsMenu,
    handleUserInput,
  ]);

  // True while any nested return-on-escape route is stashed — i.e. the user
  // drilled in from a parent menu, so Esc steps back rather than closing.
  const hasReturnStash =
    settingsReturnOnEscape ||
    verboseReturnOnEscape ||
    themeReturnOnEscape !== null;

  // /verbosity preview keymap. Gated to liteOnly commands with a preview
  // fixture so Ctrl+P / p don't fire in modern-TUI menus (Menu.tsx yields
  // Ctrl+P on liteOnly menus so this handler can claim it; plain ↑ still
  // navigates). Ctrl+P toggles hidden ↔ mini (expanded → hidden closes the
  // pane too); `p` refines mini → expanded (the expanded pane owns its own
  // `p` for the reverse).
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
      return;
    }
  });

  // Ctrl+C inside any menu surface = Esc (one level). Without this it falls
  // through to AppContainer's double-Ctrl+C quit flow — a startling
  // overreaction to backing out of a menu.
  useKeypress((input, key) => {
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
      if (!activeCommand) return;
      const opt = activeCommand.options.find((o) => o.label === item.label);
      if (!opt) return;

      // /verbosity density-menu rows: track the highlighted preset so the
      // inline preview can draft-render it. Both `menu:density:confirm:<preset>`
      // (the density menu) and `density:apply:<preset>` (the confirmation
      // submenu's Yes row) carry a preset name we want to draft. The
      // Custom / ← back / Cancel rows clear the draft so the preview
      // reverts to the saved config.
      if (activeCommand.command.name === '/verbosity') {
        const m =
          opt.value.match(/^menu:density:confirm:([a-z]+)$/) ??
          opt.value.match(/^density:apply:([a-z]+)$/);
        if (m && DENSITY_PRESETS.includes(m[1] as DensityPreset)) {
          setDraftPreset(m[1] as DensityPreset);
        } else {
          setDraftPreset(null);
        }
      }

      if (!isThemeMenu) return;

      const fallbackDiff = buildFallbackDiff({
        added: {
          background: themeColors.diff.added.background,
          bar: themeColors.diff.added.bar,
          highlight: themeColors.diff.added.highlight,
        },
        removed: {
          background: themeColors.diff.removed.background,
          bar: themeColors.diff.removed.bar,
          highlight: themeColors.diff.removed.highlight,
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
        if (theme)
          setThemePreview(
            buildBundledPreview(theme, fallbackDiff, themeColors.brand)
          );
        return;
      }

      // Custom option — show current prefs preview
      if (opt.value === 'custom') {
        const currentPrefs = loadUserThemePrefs();
        setThemePreview(
          buildCurrentPreview(currentPrefs, fallbackDiff, themeColors.brand)
        );
        return;
      }

      // Custom prompt/response/diff preset: build a preview combining the
      // highlighted preset (whichever of prompt/response/diff its value
      // names) with the saved config for the other two.
      const prefs = loadUserThemePrefs();
      for (const kind of ['prompt', 'response', 'diff'] as const) {
        if (!opt.value.startsWith(`${kind}:`)) continue;
        const presetId = opt.value.slice(kind.length + 1);
        const prompt =
          (kind === 'prompt'
            ? getPromptPreset(presetId)
            : getPromptPreset(prefs.promptPreset)) ?? promptPresets[0]!;
        const response =
          (kind === 'response'
            ? getResponsePreset(presetId)
            : getResponsePreset(prefs.responsePreset)) ?? responsePresets[0]!;
        const diff =
          (kind === 'diff'
            ? getDiffPreset(presetId)
            : getDiffPreset(prefs.diffPreset)) ?? diffPresets[0]!;
        setThemePreview(
          buildBundledPreview(
            { id: 'preview', label: 'Preview', prompt, response, diff },
            fallbackDiff,
            themeColors.brand
          )
        );
        return;
      }

      // Keep current preview for other options (Custom, Prompt style, Response text color)
    },
    [isThemeMenu, activeCommand, setThemePreview, themeColors]
  );

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

    // /verbosity truncation editor mode: previewKey ends in `:edit`. Swap
    // the regular menu for the numeric editor; the editor handles all
    // keypresses itself and routes back to the truncation submenu via
    // executeCommandWithArg on commit/cancel.
    const previewKey = activeCommand.previewKey;
    const truncEditMatch =
      previewKey &&
      previewKey.match(
        /^truncation:(argsLines|argsChars|outputLines|outputChars):edit$/
      );

    // Lite /verbosity gets the same panel chrome as the other settings: a
    // `/settings – verbosity – <sub>` breadcrumb header + divider above the
    // menu/preview/editor, deepening one level per drilldown. LiteLayout hides
    // the input row while this menu is active (see isLiteVerbosityMenu there),
    // so the breadcrumb sits where the input box was — matching display/theme/
    // terminal/etc. Gated to lite: in TUI /verbosity is filtered out entirely.
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
      const which = truncEditMatch[1] as
        | 'argsLines'
        | 'argsChars'
        | 'outputLines'
        | 'outputChars';
      // Map the editor field onto the saved-config key used by the
      // `set:<field>:<value>` setter route.
      const settingKey = (
        {
          argsLines: 'argsMaxLines',
          argsChars: 'argsMaxChars',
          outputLines: 'outputMaxLines',
          outputChars: 'outputMaxChars',
        } as const
      )[which];
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

    // Verbosity submenu preview pane: previewKey is one of the
    // VerbosityPreviewKey strings. 'truncation' is a submenu, not a fixture
    // key; map it to 'top' so a generic mix shows next to the cap rows.
    const verbosityPreviewKey: VerbosityPreviewKey | null =
      previewKey === 'truncation'
        ? 'top'
        : previewKey && VERBOSITY_PREVIEW_KEYS.has(previewKey)
          ? (previewKey as VerbosityPreviewKey)
          : null;

    // Expanded preview: swap the menu surface for the scrollable pane. The
    // pane owns its own keypresses; `p` cycles forward to hidden, Esc
    // collapses back to mini. Only meaningful inside /verbosity, but the
    // gate is enforced upstream (we only set previewMode away from 'mini'
    // when the open command is /verbosity).
    if (previewMode === 'expanded' && verbosityPreviewKey) {
      return (
        <Box flexDirection="column">
          {verbosityHeader}
          <VerbosityPreviewPane
            which={verbosityPreviewKey}
            displayOverride={
              draftPreset ? DENSITY_DISPLAY[draftPreset] : undefined
            }
            filtersOverride={
              draftPreset ? DENSITY_FILTERS[draftPreset] : undefined
            }
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
          onHighlight={handleActiveCommandHighlight}
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
          closeMenuActionLabel={hasReturnStash ? '← back' : 'to close'}
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
        {verbosityPreviewKey && previewMode === 'mini' && (
          <VerbosityPreview
            which={verbosityPreviewKey}
            displayOverride={
              draftPreset ? DENSITY_DISPLAY[draftPreset] : undefined
            }
            filtersOverride={
              draftPreset ? DENSITY_FILTERS[draftPreset] : undefined
            }
          />
        )}
        {verbosityPreviewKey && (
          <Box paddingX={1}>
            <Text>
              {secondaryColor(
                previewMode === 'hidden'
                  ? '  ctrl+p to show preview'
                  : '  p to expand · ctrl+p to hide preview'
              )}
            </Text>
          </Box>
        )}
      </Box>
    );
  }

  return null;
};
