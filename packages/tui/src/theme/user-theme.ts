/**
 * User theme customization — color presets and persistence.
 *
 * Stores user-chosen prompt and response styles in ~/.kiro/settings/kiro_cli_theme.json.
 * All colors are sourced from existing kiroDark/kiroLight theme definitions.
 * Prompt presets are text+background combos. Response presets are text-only.
 */

import { readFileSync, writeFileSync, mkdirSync } from 'fs';
import { join } from 'path';
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

/** Persisted user theme preferences */
export interface UserThemePrefs {
  promptPreset?: string;
  responsePreset?: string;
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
    bgColor: { truecolor: '#552B99', color256: 57 },
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

/** A bundled theme applies both prompt and response styles in one shot */
export interface BundledTheme {
  id: string;
  label: string;
  prompt: PromptPreset;
  response: ResponsePreset;
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
  },
];

const PROMPT_PREVIEW = 'This is how your prompt will look';
const RESPONSE_PREVIEW = 'This is how the response will look';

/** Build a combined preview for a bundled theme showing prompt and response on separate lines */
export function buildBundledPreview(theme: BundledTheme): string {
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

  return `${promptPart}\n${responsePart}`;
}

/** Look up a bundled theme by id */
export function getBundledTheme(
  id: string | undefined
): BundledTheme | undefined {
  if (!id) return undefined;
  return bundledThemes.find((t) => t.id === id);
}

/** Build a preview from current prefs (for custom flow — shows what the user currently has) */
export function buildCurrentPreview(prefs: UserThemePrefs): string {
  const prompt = getPromptPreset(prefs.promptPreset) ?? promptPresets[0]!;
  const response =
    getResponsePreset(prefs.responsePreset) ?? responsePresets[0]!;
  // Reuse bundled preview logic with an ad-hoc bundled theme
  return buildBundledPreview({
    id: 'current',
    label: 'Current',
    prompt,
    response,
  });
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
  const home =
    process.env.HOME || process.env.USERPROFILE || require('os').homedir();
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
