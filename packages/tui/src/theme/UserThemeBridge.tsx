/**
 * Bridge component that connects ThemeProvider's setUserColors to the Zustand store.
 * Also registers a getter for the base theme's diff hex colors (used for preview fallback).
 * Must be rendered inside both ThemeProvider and AppStoreProvider.
 */

import { useEffect, useCallback } from 'react';
import { useTheme } from '../hooks/useThemeContext.js';
import { useAppStore } from '../stores/app-store.js';
import chalk from 'chalk';
import type { Theme } from './types.js';
import {
  PROMPT_PREVIEW,
  RESPONSE_PREVIEW,
  DIFF_ADDED_PREVIEW,
  DIFF_REMOVED_PREVIEW,
  DIFF_HEADER,
} from './user-theme.js';

/** @internal Exported for testing */
export function extractThemeDiffHex(getColor: (path: string) => any): {
  added: { background: string; bar: string; highlight: string };
  removed: { background: string; bar: string; highlight: string };
} {
  return {
    added: {
      background: getColor('diff.added.background').hex as string,
      bar: getColor('diff.added.bar').hex as string,
      highlight: getColor('diff.added.highlight').hex as string,
    },
    removed: {
      background: getColor('diff.removed.background').hex as string,
      bar: getColor('diff.removed.bar').hex as string,
      highlight: getColor('diff.removed.highlight').hex as string,
    },
  };
}

/** @internal Exported for testing */
export function buildAutoPreview(colors: Theme['colors']): string {
  const surfaceHex = colors.surface.truecolor;
  const addedBgHex = colors.diff.added.background.truecolor;
  const addedBarHex = colors.diff.added.bar.truecolor;
  const removedBgHex = colors.diff.removed.background.truecolor;
  const removedBarHex = colors.diff.removed.bar.truecolor;

  // Prompt: terminal default text on surface background
  const bg = surfaceHex ? chalk.bgHex(surfaceHex) : (s: string) => s;
  const promptPart = bg(` ${PROMPT_PREVIEW} `);

  // Response: terminal default text
  const responsePart = RESPONSE_PREVIEW;

  // Diff
  const addedBg = addedBgHex ? chalk.bgHex(addedBgHex) : (s: string) => s;
  const removedBg = removedBgHex ? chalk.bgHex(removedBgHex) : (s: string) => s;
  const addedBar = addedBarHex ? chalk.hex(addedBarHex) : (s: string) => s;
  const removedBar = removedBarHex
    ? chalk.hex(removedBarHex)
    : (s: string) => s;

  const addedLine = addedBg(addedBar(DIFF_ADDED_PREVIEW));
  const removedLine = removedBg(removedBar(DIFF_REMOVED_PREVIEW));

  return `${promptPart}\n${responsePart}\n\n${DIFF_HEADER}\n${addedLine}\n${removedLine}`;
}

export const UserThemeBridge = () => {
  const { setUserColors, setBaseTheme, getColor, baseTheme } = useTheme();
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
    () => extractThemeDiffHex(getColor),
    [getColor]
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
