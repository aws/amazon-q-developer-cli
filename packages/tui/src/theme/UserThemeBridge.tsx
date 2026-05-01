/**
 * Bridge component that connects ThemeProvider's setUserColors to the Zustand store.
 * Also registers a getter for the base theme's diff hex colors (used for preview fallback).
 * Must be rendered inside both ThemeProvider and AppStoreProvider.
 */

import { useEffect, useCallback } from 'react';
import { useTheme } from '../hooks/useThemeContext.js';
import { useAppStore } from '../stores/app-store.js';
import type { Theme } from './types.js';
import type { TerminalColor } from '../types/themeTypes.js';
import {
  PROMPT_PREVIEW,
  RESPONSE_PREVIEW,
  DIFF_ADDED_PREVIEW,
  DIFF_REMOVED_PREVIEW,
  DIFF_HEADER,
  chalkFromTerminalColor,
} from './user-theme.js';

/** @internal Exported for testing */
export function extractThemeDiffColors(colors: Theme['colors']): {
  added: {
    background: TerminalColor;
    bar: TerminalColor;
    highlight: TerminalColor;
  };
  removed: {
    background: TerminalColor;
    bar: TerminalColor;
    highlight: TerminalColor;
  };
} {
  return {
    added: {
      background: colors.diff.added.background,
      bar: colors.diff.added.bar,
      highlight: colors.diff.added.highlight,
    },
    removed: {
      background: colors.diff.removed.background,
      bar: colors.diff.removed.bar,
      highlight: colors.diff.removed.highlight,
    },
  };
}

/** @internal Exported for testing */
export function buildAutoPreview(colors: Theme['colors']): string {
  // Prompt: terminal default text on surface background
  const bg = chalkFromTerminalColor(colors.surface, 'bg');
  const promptPart = bg(` ${PROMPT_PREVIEW} `);

  // Response: terminal default text
  const responsePart = RESPONSE_PREVIEW;

  // Diff
  const addedBg = chalkFromTerminalColor(colors.diff.added.background, 'bg');
  const removedBg = chalkFromTerminalColor(
    colors.diff.removed.background,
    'bg'
  );
  const addedBar = chalkFromTerminalColor(colors.diff.added.bar, 'fg');
  const removedBar = chalkFromTerminalColor(colors.diff.removed.bar, 'fg');

  // Use foreground color for the whole line when set (kiroSafe style), otherwise just color the bar
  const addedFg = colors.diff.added.foreground
    ? chalkFromTerminalColor(colors.diff.added.foreground, 'fg')
    : undefined;
  const removedFg = colors.diff.removed.foreground
    ? chalkFromTerminalColor(colors.diff.removed.foreground, 'fg')
    : undefined;

  const addedLine = addedFg
    ? addedBg(addedFg(DIFF_ADDED_PREVIEW))
    : addedBg(addedBar(DIFF_ADDED_PREVIEW));
  const removedLine = removedFg
    ? removedBg(removedFg(DIFF_REMOVED_PREVIEW))
    : removedBg(removedBar(DIFF_REMOVED_PREVIEW));

  return `${promptPart}\n${responsePart}\n\n${DIFF_HEADER}\n${addedLine}\n${removedLine}`;
}

export const UserThemeBridge = () => {
  const { setUserColors, setBaseTheme, baseTheme } = useTheme();
  const registerUserColorsSetter = useAppStore(
    (state) => state.registerUserColorsSetter
  );
  const registerBaseThemeSetter = useAppStore(
    (state) => state.registerBaseThemeSetter
  );
  const registerThemeDiffHexGetter = useAppStore(
    (state) => state.registerThemeDiffHexGetter
  );
  const registerAutoPreviewGetter = useAppStore(
    (state) => state.registerAutoPreviewGetter
  );

  const getThemeDiffHex = useCallback(
    () => extractThemeDiffColors(baseTheme.colors),
    [baseTheme]
  );

  const getAutoPreview = useCallback(() => {
    return buildAutoPreview(baseTheme.colors);
  }, [baseTheme]);

  useEffect(() => {
    registerUserColorsSetter(setUserColors);
  }, [setUserColors, registerUserColorsSetter]);

  useEffect(() => {
    registerBaseThemeSetter(setBaseTheme);
  }, [setBaseTheme, registerBaseThemeSetter]);

  useEffect(() => {
    registerThemeDiffHexGetter(getThemeDiffHex);
  }, [getThemeDiffHex, registerThemeDiffHexGetter]);

  useEffect(() => {
    registerAutoPreviewGetter(getAutoPreview);
  }, [getAutoPreview, registerAutoPreviewGetter]);

  return null;
};
