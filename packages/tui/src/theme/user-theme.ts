/**
 * User theme customization — color presets and persistence.
 *
 * Stores user-chosen prompt and response styles in ~/.kiro/settings/kiro_cli_theme.json.
 * All colors are sourced from existing kiroDark/kiroLight theme definitions.
 * Prompt presets are text+background combos. Response presets are text-only.
 */

import { readFileSync, writeFileSync, mkdirSync } from 'fs';
import { join } from 'path';
import { homedir } from 'os';
import chalk from 'chalk';
import type { TerminalColor } from '../types/themeTypes.js';
import { logger } from '../utils/logger.js';

/** A prompt preset pairs a text color with a background color */
export interface PromptPreset {
  id: string;
  label: string;
  textColor: TerminalColor;
  bgColor: TerminalColor;
}

/** A response preset is text color only */
export interface ResponsePreset {
  id: string;
  label: string;
  textColor: TerminalColor;
}

/** A diff color preset defines added/removed line colors */
export interface DiffPreset {
  id: string;
  label: string;
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
}

/** Persisted user theme preferences */
export interface UserThemePrefs {
  promptPreset?: string;
  responsePreset?: string;
  diffPreset?: string;
  /** Base theme override: 'dark', 'light', or undefined (auto-detect) */
  baseTheme?: 'dark' | 'light';
}

// All colors below are taken directly from kiroDark.ts / kiroLight.ts

/** Prompt presets — text + background combos using only existing theme colors */
export const promptPresets: PromptPreset[] = [
  // default: uses theme surface — bgColor is a placeholder, overridden at preview/render time
  {
    id: 'default',
    label: 'Default',
    textColor: { named: 'default' },
    bgColor: { named: 'default' },
  },
  // white text on snackbar purple bg
  {
    id: 'purple',
    label: 'Purple',
    textColor: { truecolor: '#ffffff', color256: 15 },
    bgColor: { truecolor: '#552B99', color256: 54 },
  },
  // cyan text on dark surface
  {
    id: 'ocean',
    label: 'Ocean',
    textColor: { truecolor: '#80F4FF', color256: 123 },
    bgColor: { truecolor: '#262626', color256: 235 },
  },
  // green text on dark surface
  {
    id: 'forest',
    label: 'Forest',
    textColor: { truecolor: '#80FFB5', color256: 121 },
    bgColor: { truecolor: '#262626', color256: 235 },
  },
  // black text on light surface (kiroLight.surface) — light mode feel
  {
    id: 'paper',
    label: 'Paper',
    textColor: { truecolor: '#000000', color256: 0 },
    bgColor: { truecolor: '#EEEEEE', color256: 255 },
  },
];

/** Response presets — text color only, all from existing theme palette */
export const responsePresets: ResponsePreset[] = [
  { id: 'default', label: 'Default', textColor: { named: 'default' } },
  {
    id: 'light',
    label: 'Light',
    textColor: { truecolor: '#FFFFFF', color256: 255 },
  }, // syntax.subst (white)
  {
    id: 'dark',
    label: 'Dark',
    textColor: { truecolor: '#626262', color256: 241 },
  }, // muted
];

/** Diff presets — covers dark bg, light bg, and colorblind-friendly palettes */
export const diffPresets: DiffPreset[] = [
  {
    id: 'default',
    label: 'Default',
    added: {
      background: { named: 'default' },
      bar: { named: 'default' },
      highlight: { named: 'default' },
    },
    removed: {
      background: { named: 'default' },
      bar: { named: 'default' },
      highlight: { named: 'default' },
    },
  },
  {
    // Classic green/red on dark background — matches kiroDark
    id: 'dark',
    label: 'Dark',
    added: {
      background: { truecolor: '#2d3a30', color256: 22 },
      bar: { truecolor: '#80ffb5', color256: 121 },
      highlight: { truecolor: '#2d3a30', color256: 22 },
    },
    removed: {
      background: { truecolor: '#3a2d2f', color256: 52 },
      bar: { truecolor: '#ff8080', color256: 210 },
      highlight: { truecolor: '#3a2d2f', color256: 52 },
    },
  },
  {
    // Green/red on light background — matches kiroLight with stronger contrast
    id: 'light',
    label: 'Light',
    added: {
      background: { truecolor: '#d4f0d4', color256: 194 },
      bar: { truecolor: '#00875F', color256: 35 },
      highlight: { truecolor: '#c0e8c0', color256: 157 },
    },
    removed: {
      background: { truecolor: '#f0d4d4', color256: 224 },
      bar: { truecolor: '#d94d4d', color256: 167 },
      highlight: { truecolor: '#e8c2c2', color256: 217 },
    },
  },
  {
    // Blue/pink — deuteranopia & protanopia safe, dark background
    id: 'colorblind-dark',
    label: 'Accessible Dark',
    added: {
      background: { truecolor: '#1a2a3a', color256: 17 },
      bar: { truecolor: '#80F4FF', color256: 123 },
      highlight: { truecolor: '#1a2a3a', color256: 17 },
    },
    removed: {
      background: { truecolor: '#3a2a2e', color256: 52 },
      bar: { truecolor: '#FFAFD1', color256: 218 },
      highlight: { truecolor: '#3a2a2e', color256: 52 },
    },
  },
  {
    // Blue/pink — deuteranopia & protanopia safe, light background
    id: 'colorblind-light',
    label: 'Accessible Light',
    added: {
      background: { truecolor: '#d4e4f0', color256: 189 },
      bar: { truecolor: '#5aa3d9', color256: 74 },
      highlight: { truecolor: '#d4e4f0', color256: 189 },
    },
    removed: {
      background: { truecolor: '#f0d4e0', color256: 224 },
      bar: { truecolor: '#d94d8a', color256: 168 },
      highlight: { truecolor: '#f0d4e0', color256: 224 },
    },
  },
];

/** A bundled theme applies both prompt and response styles in one shot */
export interface BundledTheme {
  id: string;
  label: string;
  prompt: PromptPreset;
  response: ResponsePreset;
  diff: DiffPreset;
}

/** Bundled themes — one-click combos for Light and Dark */
export const bundledThemes: BundledTheme[] = [
  {
    id: 'dark',
    label: 'Dark Theme',
    prompt: {
      id: 'default',
      label: 'Default',
      textColor: { named: 'default' },
      bgColor: { truecolor: '#262626', color256: 235 },
    },
    response: {
      id: 'light',
      label: 'Light',
      textColor: { truecolor: '#FFFFFF', color256: 255 },
    },
    diff: diffPresets.find((p) => p.id === 'dark')!,
  },
  {
    id: 'light',
    label: 'Light Theme',
    prompt: {
      id: 'paper',
      label: 'Paper',
      textColor: { truecolor: '#000000', color256: 0 },
      bgColor: { truecolor: '#EEEEEE', color256: 255 },
    },
    response: {
      id: 'dark',
      label: 'Dark',
      textColor: { truecolor: '#626262', color256: 241 },
    },
    diff: diffPresets.find((p) => p.id === 'light')!,
  },
];

export const PROMPT_PREVIEW = 'This is how your prompt will look';
export const RESPONSE_PREVIEW = 'This is how the response will look';
export const DIFF_ADDED_PREVIEW = '+  const result = compute(input);';
export const DIFF_REMOVED_PREVIEW = '-  const result = calculate(input);';
export const DIFF_HEADER = chalk.gray(
  'Code diff — added and removed lines will look like:'
);

/** Build a short diff preview showing one added and one removed line.
 *  @param fallbackDiff - base theme diff colors, used when preset is 'default'
 */
export function buildDiffPreview(
  preset: DiffPreset,
  currentId?: string,
  fallbackDiff?: DiffPreset
): string {
  const marker = preset.id === (currentId ?? 'default') ? '  ✓' : '';
  // Resolve effective preset: if 'default', use fallback (base theme colors)
  const effective =
    preset.added.bar.named === 'default' && fallbackDiff
      ? fallbackDiff
      : preset;
  // If still no truecolor values, render plain
  if (!effective.added.bar.truecolor) {
    return `\n${DIFF_HEADER}\n${DIFF_ADDED_PREVIEW}\n${DIFF_REMOVED_PREVIEW}${marker}`;
  }
  const addedBg = effective.added.background.truecolor
    ? chalk.bgHex(effective.added.background.truecolor)
    : (s: string) => s;
  const addedBar = effective.added.bar.truecolor
    ? chalk.hex(effective.added.bar.truecolor)
    : (s: string) => s;
  const removedBg = effective.removed.background.truecolor
    ? chalk.bgHex(effective.removed.background.truecolor)
    : (s: string) => s;
  const removedBar = effective.removed.bar.truecolor
    ? chalk.hex(effective.removed.bar.truecolor)
    : (s: string) => s;

  const addedLine = addedBg(`${addedBar(DIFF_ADDED_PREVIEW)}`);
  const removedLine = removedBg(`${removedBar(DIFF_REMOVED_PREVIEW)}`);
  return `\n${DIFF_HEADER}\n${addedLine}\n${removedLine}${marker}`;
}

/** Build a combined preview for a bundled theme showing prompt, response, and diff.
 *  @param fallbackDiff - base theme diff colors, used when diff preset is 'default'
 */
export function buildBundledPreview(
  theme: BundledTheme,
  fallbackDiff?: DiffPreset
): string {
  const bgHex = theme.prompt.bgColor.truecolor ?? '#262626';
  const bg = chalk.bgHex(bgHex);
  const promptText = ` ${PROMPT_PREVIEW} `;
  let promptPart: string;
  if (theme.prompt.textColor.named === 'default') {
    promptPart = bg(promptText);
  } else {
    const fgHex = theme.prompt.textColor.truecolor;
    promptPart = fgHex ? bg(chalk.hex(fgHex)(promptText)) : bg(promptText);
  }

  let responsePart: string;
  if (theme.response.textColor.named === 'default') {
    responsePart = RESPONSE_PREVIEW;
  } else {
    const hex = theme.response.textColor.truecolor;
    responsePart = hex ? chalk.hex(hex)(RESPONSE_PREVIEW) : RESPONSE_PREVIEW;
  }

  const diffPart = buildDiffPreview(theme.diff, undefined, fallbackDiff);

  return `${promptPart}\n${responsePart}\n${diffPart}`;
}

/** Look up a bundled theme by id */
export function getBundledTheme(
  id: string | undefined
): BundledTheme | undefined {
  if (!id) return undefined;
  return bundledThemes.find((t) => t.id === id);
}

/** Build a preview from current prefs (for custom flow — shows what the user currently has).
 *  @param fallbackDiff - base theme diff colors, used when diff preset is 'default'
 */
export function buildCurrentPreview(
  prefs: UserThemePrefs,
  fallbackDiff?: DiffPreset
): string {
  const prompt = getPromptPreset(prefs.promptPreset) ?? promptPresets[0]!;
  const response =
    getResponsePreset(prefs.responsePreset) ?? responsePresets[0]!;
  const diff = getDiffPreset(prefs.diffPreset) ?? diffPresets[0]!;
  return buildBundledPreview(
    {
      id: 'current',
      label: 'Current',
      prompt,
      response,
      diff,
    },
    fallbackDiff
  );
}

/**
 * Build a preview showing sample prompt text with the combo's text + background colors.
 * @param themeSurfaceHex - the current theme's surface hex, used for the 'default' preset
 */
export function buildPromptPreview(
  preset: PromptPreset,
  currentId?: string,
  themeSurfaceHex?: string
): string {
  const marker = preset.id === (currentId ?? 'default') ? '  ✓' : '';
  // For default preset, use the theme's actual surface color
  const bgHex =
    preset.id === 'default' && themeSurfaceHex
      ? themeSurfaceHex
      : (preset.bgColor.truecolor ?? '#262626');
  const bg = chalk.bgHex(bgHex);
  const text = ` ${PROMPT_PREVIEW} `;
  if (preset.textColor.named === 'default') return bg(text) + marker;
  const fgHex = preset.textColor.truecolor;
  if (fgHex) return bg(chalk.hex(fgHex)(text)) + marker;
  if (preset.textColor.named)
    return bg((chalk as any)[preset.textColor.named]?.(text) ?? text) + marker;
  return bg(text) + marker;
}

/** Build a preview showing sample response text in the preset's color */
export function buildResponsePreview(
  preset: ResponsePreset,
  currentId?: string
): string {
  const marker = preset.id === (currentId ?? 'default') ? '  ✓' : '';
  if (preset.textColor.named === 'default') return RESPONSE_PREVIEW + marker;
  const hex = preset.textColor.truecolor;
  if (hex) return chalk.hex(hex)(RESPONSE_PREVIEW) + marker;
  if (preset.textColor.named)
    return (
      ((chalk as any)[preset.textColor.named]?.(RESPONSE_PREVIEW) ??
        RESPONSE_PREVIEW) + marker
    );
  return RESPONSE_PREVIEW + marker;
}

function getThemePath(): string {
  const home = process.env.HOME || process.env.USERPROFILE || homedir();
  return join(home, '.kiro', 'settings', 'kiro_cli_theme.json');
}

/** Load user theme prefs from disk. Returns empty object on missing/invalid file. */
export function loadUserThemePrefs(): UserThemePrefs {
  try {
    const raw = readFileSync(getThemePath(), 'utf-8');
    const parsed = JSON.parse(raw);
    if (typeof parsed === 'object' && parsed !== null) {
      return parsed as UserThemePrefs;
    }
  } catch {
    // File doesn't exist or is invalid — use defaults
  }
  return {};
}

/** Save user theme prefs to disk. Returns true on success. */
export function saveUserThemePrefs(prefs: UserThemePrefs): boolean {
  try {
    const themePath = getThemePath();
    mkdirSync(join(themePath, '..'), { recursive: true });
    writeFileSync(themePath, JSON.stringify(prefs, null, 2) + '\n');
    return true;
  } catch (err) {
    logger.error('[user-theme] Failed to save theme prefs:', err);
    return false;
  }
}

/** Look up a prompt preset by id */
export function getPromptPreset(
  id: string | undefined
): PromptPreset | undefined {
  if (!id) return undefined;
  return promptPresets.find((p) => p.id === id);
}

/** Look up a response preset by id */
export function getResponsePreset(
  id: string | undefined
): ResponsePreset | undefined {
  if (!id) return undefined;
  return responsePresets.find((p) => p.id === id);
}

/** Look up a diff preset by id */
export function getDiffPreset(id: string | undefined): DiffPreset | undefined {
  if (!id) return undefined;
  return diffPresets.find((p) => p.id === id);
}

/** Build a DiffPreset from raw theme diff hex values (for use as fallback in previews) */
export function buildFallbackDiff(themeDiff: {
  added: { background: string; bar: string; highlight: string };
  removed: { background: string; bar: string; highlight: string };
}): DiffPreset {
  return {
    id: '_fallback',
    label: 'Theme default',
    added: {
      background: { truecolor: themeDiff.added.background },
      bar: { truecolor: themeDiff.added.bar },
      highlight: { truecolor: themeDiff.added.highlight },
    },
    removed: {
      background: { truecolor: themeDiff.removed.background },
      bar: { truecolor: themeDiff.removed.bar },
      highlight: { truecolor: themeDiff.removed.highlight },
    },
  };
}
