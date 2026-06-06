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
import type { VerbosityPreviewKey } from '../../lite/render.js';
import {
  DENSITY_DISPLAY,
  DENSITY_FILTERS,
  DENSITY_PRESETS,
  type DensityPreset,
} from '../../lite/verbose.js';

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

  // /verbosity preview mode — three states with two distinct keys:
  //
  //   'hidden'   — default. Preview suppressed entirely; menu reads as a
  //                clean settings list. Ctrl+P opens the inline (mini)
  //                preview from here.
  //   'mini'     — Inline truncated preview below the menu. `p` from here
  //                expands to the full pane; Ctrl+P returns to hidden.
  //   'expanded' — Full-height scrollable pane. Replaces the menu surface;
  //                `p` collapses back to mini, Ctrl+P returns to hidden.
  //
  // Two-key design: Ctrl+P is the master switch (hidden ↔ mini), `p` is the
  // refine toggle (mini ↔ expanded). This way the user can opt-in to the
  // preview once and never see a chord again unless they want the pane,
  // and pressing `p` while typing in a non-preview menu can't accidentally
  // pop a preview open.
  type PreviewMode = 'mini' | 'expanded' | 'hidden';
  const [previewMode, setPreviewMode] = useState<PreviewMode>('hidden');

  // Highlighted preset in the /verbosity density menu (or confirmation
  // submenu). Used to draft-preview the preset's display + filters before
  // the user commits — the preview pane reads this and feeds the draft
  // overrides into VerbosityPreview.
  //
  // null when the cursor isn't on a preset row (e.g. on Custom or ← back),
  // or when the active menu isn't density-shaped at all.
  const [draftPreset, setDraftPreset] = useState<DensityPreset | null>(null);

  // Force the Menu to remount whenever the menu's *shape* changes (different
  // command, different option set), so the cursor gets re-clamped to a
  // valid row and `initialIndex` re-applies. We deliberately avoid bumping
  // the key when only descriptions change (toggle re-opens of the same
  // submenu produce identical option values), so the cursor stays on the
  // row the user just toggled.
  // Computed during render so the new key applies on the same frame as the
  // new options — avoids a one-frame flicker of the old cursor on the new
  // menu.
  const menuShapeKey = useMemo(() => {
    if (!activeCommand) return '';
    return `${activeCommand.command.name}|${activeCommand.options.map((o) => o.value).join(',')}`;
  }, [activeCommand]);
  const lastShapeKeyRef = useRef('');
  const shapeVersionRef = useRef(0);
  if (lastShapeKeyRef.current !== menuShapeKey) {
    lastShapeKeyRef.current = menuShapeKey;
    if (menuShapeKey) shapeVersionRef.current += 1;
  }
  // Reset the preview state ONLY when leaving /verbosity entirely (i.e. the
  // user navigates to a different command, or the overlay closes). Within
  // /verbosity, preview state persists across submenu switches — once the
  // user arms the preview with Ctrl+P, walking from density → tool → output
  // shouldn't make the panel disappear and force a re-arm. The fixture key
  // changes per submenu, but every /verbosity submenu has a previewKey, so
  // the same preview UI just re-renders against the new fixture.
  //
  // draftPreset (highlighted density preset) is also gated on /verbosity:
  // outside that command nothing reads it, so we proactively null it out so
  // a stale preset can't leak into a future /verbosity session. Inside the
  // command, handleActiveCommandHighlight clears it on its own when the
  // cursor lands on a non-preset row, so we don't need to touch it here for
  // intra-/verbosity transitions.
  const activeCommandName = activeCommand?.command.name ?? null;
  useEffect(() => {
    if (activeCommandName !== '/verbosity') {
      if (previewMode !== 'hidden') setPreviewMode('hidden');
      if (draftPreset !== null) setDraftPreset(null);
    }
    // Depend only on the command name so submenu shape changes within
    // /verbosity (which keep activeCommandName stable) don't trigger a
    // reset. previewMode/draftPreset are intentionally NOT in the deps —
    // we read them but only act on the command-leaving transition.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeCommandName]);
  // The effective Menu key combines the shape version with `initialIndex`
  // — re-entering the top menu via ESC at a different row should still
  // remount even though the option set is unchanged.
  const activeCommandKey = `${shapeVersionRef.current}:${activeCommand?.initialIndex ?? 0}`;

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

  // Close the activeCommand overlay (one level). Honors the /settings,
  // /verbose, and /theme return-on-escape stashes so drilling into a
  // sub-menu and pressing Esc lands the user back in the parent menu
  // instead of dropping the whole overlay.
  const handleActiveCommandClose = useCallback(() => {
    // If this overlay was opened from /settings (e.g. the user is now in the
    // /theme menu reached via /settings → Theme), ESC should return to the
    // /settings top-level menu rather than dismiss the whole overlay. The flag
    // is set by the /settings subcommand handlers and consumed (and cleared)
    // here.
    const returnToSettings = settingsReturnOnEscape;
    // Same idea for /verbose sub-menus — the verbose handler stashes the
    // parent route (e.g. 'menu:top') when it opens any non-root sub-menu, and
    // we re-dispatch /verbose with that route so the user lands one level up
    // rather than dropping out of the menu entirely. Cleared on consume; the
    // verbose handler clears it when the user reaches the top menu so the
    // next ESC fully exits.
    const verboseReturn = verboseReturnOnEscape;
    // /theme has the same multi-level shape (top → custom → prompt|response
    // |diff). Empty string is the "go back to bare /theme" route — distinct
    // from `null`, which means "close the overlay". Consume by re-dispatching
    // `/theme [route]`. Priority: theme > settings > verbose, so a user who
    // entered /theme via /settings → Theme can still escape one /theme level
    // at a time before settings takes over.
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
    if (returnToSettings) {
      setSettingsReturnOnEscape(false);
      // Re-open /settings directly. Going through handleUserInput here caused
      // the process to exit for reasons not fully understood — likely races
      // with the in-flight overlay close.
      reopenSettingsMenu();
      return;
    }
    if (verboseReturn) {
      setVerboseReturnOnEscape(null);
      // Re-dispatch /verbosity with the saved parent route. We lean on the
      // existing handleUserInput pipeline because the verbosity handler is
      // registered there and unlike /settings we don't race a panel close —
      // the menu just reopens.
      handleUserInput(`/verbosity ${verboseReturn}`);
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

  // /verbosity preview keymap (only when activeCommand is /verbosity and a
  // preview fixture is available):
  //   Ctrl+P  master switch: hidden ↔ mini. Always available.
  //   p       refine toggle: mini ↔ expanded. No-op while hidden — the user
  //           must opt in via Ctrl+P first. The expanded pane handles its
  //           own `p` to collapse back to mini, so that case is already
  //           gated below by the previewMode check.
  //
  // Gated to liteOnly active commands so Ctrl+P / p don't fire in modern-
  // TUI menus (where they'd have no preview to control anyway). Menu.tsx's
  // useKeypress yields Ctrl+P on liteOnly menus so this handler can claim
  // it; plain ↑ still navigates the menu in those cases.
  useKeypress((input, key) => {
    const isLiteMenu =
      activeCommand?.command.meta?.liteOnly === true &&
      activeCommand.previewKey;
    if (!isLiteMenu) return;
    if (key.ctrl && (input === 'p' || input === 'P')) {
      // Master toggle. From any state, Ctrl+P returns to the opposite of
      // hidden. Expanded → hidden so the pane closes too (otherwise the
      // user would have to press Esc + Ctrl+P).
      setPreviewMode((m) => (m === 'hidden' ? 'mini' : 'hidden'));
      return;
    }
    if (
      previewMode !== 'expanded' &&
      previewMode !== 'hidden' &&
      (input === 'p' || input === 'P')
    ) {
      // Refine: mini → expanded. Suppressed in hidden (no preview to
      // refine) and expanded (the pane handles its own `p` for the
      // reverse direction).
      setPreviewMode('expanded');
      return;
    }
  });

  // Ctrl+C inside any menu surface = Esc. Without this, Ctrl+C falls through
  // to AppContainer's quit handler and triggers the double-Ctrl+C exit flow,
  // which is a startling overreaction to "back out of a menu". One press
  // closes one level (matches the menu's own Esc handlers).
  useKeypress((input, key) => {
    if (!(key.ctrl && input === 'c')) return;
    if (previewMode === 'expanded') {
      // In expanded preview mode, Ctrl+C collapses back to the menu
      // (matches the pane's own Esc behavior) — falling through to
      // handleActiveCommandClose would dump the whole /verbosity menu.
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
            fallbackDiff,
            themeColors.brand
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
            fallbackDiff,
            themeColors.brand
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
            fallbackDiff,
            themeColors.brand
          )
        );
        return;
      }

      // Keep current preview for other options (Custom, Prompt style, Response text color)
    },
    [isThemeMenu, activeCommand, setThemePreview, getColor, themeColors]
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
      );
    }

    // Verbosity submenu preview pane: previewKey is one of the
    // VerbosityPreviewKey strings (top/density/tool/...).
    const verbosityPreviewKey: VerbosityPreviewKey | null =
      previewKey === 'top' ||
      previewKey === 'density' ||
      previewKey === 'tool' ||
      previewKey === 'subagent' ||
      previewKey === 'output' ||
      previewKey === 'truncation' ||
      previewKey === 'truncation:args' ||
      previewKey === 'truncation:output'
        ? // 'truncation' is a submenu, not a fixture key; map to 'top' fixtures
          // since we want a generic mix shown next to the cap rows.
          previewKey === 'truncation'
          ? 'top'
          : (previewKey as VerbosityPreviewKey)
        : null;

    // Expanded preview: swap the menu surface for the scrollable pane. The
    // pane owns its own keypresses; `p` cycles forward to hidden, Esc
    // collapses back to mini. Only meaningful inside /verbosity, but the
    // gate is enforced upstream (we only set previewMode away from 'mini'
    // when the open command is /verbosity).
    if (previewMode === 'expanded' && verbosityPreviewKey) {
      return (
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
      );
    }

    return (
      <Box flexDirection="column">
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
          closeMenuActionLabel={
            settingsReturnOnEscape ||
            verboseReturnOnEscape ||
            themeReturnOnEscape !== null
              ? '← back'
              : 'to close'
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
