/**
 * Theme wizard — pure logic for the /settings → theme flow.
 *
 * The Panel component (`ThemePanel.tsx`) drives navigation; this module owns
 * the state machine + per-step config so unit tests can exercise wizard
 * transitions without rendering the UI.
 *
 * Flow per the /settings UX spec:
 *
 *     top-level: Auto · Dark theme · Light theme · Custom
 *                                                     │
 *                                              ┌──────┘
 *                                              ▼
 *                                Step 1: prompt style ─→ Step 2: response
 *                                                            │
 *                                                            ▼
 *                                                       Step 3: diff colors
 *                                                            │
 *                                                            ▼
 *                                                       finish + close
 *
 * Auto / Dark / Light apply immediately and close. Custom enters Step 1.
 * ESC at any wizard step backs up one level (Step 1 → top-level).
 */

import {
  bundledThemes,
  diffPresets,
  getDiffPreset,
  getPromptPreset,
  getResponsePreset,
  promptPresets,
  responsePresets,
  saveUserThemePrefs,
  type DiffPreset,
  type PromptPreset,
  type ResponsePreset,
  type UserThemePrefs,
} from '../theme/user-theme.js';
import { kiroDark } from '../theme/kiroDark.js';
import { kiroLight } from '../theme/kiroLight.js';

// ─── Top-level options ───────────────────────────────────────────────

export type TopLevelChoice =
  | 'auto'
  | 'bundled-dark'
  | 'bundled-light'
  | 'custom';

export interface TopLevelItem {
  choice: TopLevelChoice;
  label: string;
  /** Hint shown next to the item; '[active]' for the currently saved choice. */
  description: string;
}

/**
 * Build the top-level Auto / Dark / Light / Custom list with `[active]`
 * marker on the currently saved choice. Description hints follow the spec
 * — only Custom carries an explanatory line; the others are self-evident.
 */
export function buildTopLevelItems(prefs: UserThemePrefs): TopLevelItem[] {
  const activeBundled = bundledThemes.find((t) => {
    const matchPrompt = (prefs.promptPreset ?? 'default') === t.prompt.id;
    const matchResponse = (prefs.responsePreset ?? 'default') === t.response.id;
    const matchDiff = (prefs.diffPreset ?? 'default') === t.diff.id;
    return matchPrompt && matchResponse && matchDiff;
  })?.id;
  const isCustomActive =
    !activeBundled &&
    Boolean(prefs.promptPreset || prefs.responsePreset || prefs.diffPreset);
  const isAutoActive = !activeBundled && !isCustomActive;

  return [
    {
      choice: 'auto',
      label: 'Auto',
      description: isAutoActive ? '[active]' : '',
    },
    {
      choice: 'bundled-dark',
      label: 'Dark theme',
      description: activeBundled === 'dark' ? '[active]' : '',
    },
    {
      choice: 'bundled-light',
      label: 'Light theme',
      description: activeBundled === 'light' ? '[active]' : '',
    },
    {
      choice: 'custom',
      label: 'Custom',
      description: isCustomActive
        ? '[active]'
        : 'Choose prompt, response and diff colors separately',
    },
  ];
}

// ─── Wizard step config ──────────────────────────────────────────────
//
// Order of `wizardSteps` defines the wizard sequence. To add a step,
// append an entry — `nextStep`, `prevStep`, and the panel's renderer
// pick it up automatically.

export type WizardStepId = 'prompt' | 'response' | 'diff';

export interface WizardStep {
  readonly id: WizardStepId;
  /** Used in error messages: "Unknown <label> preset: ..." */
  readonly label: string;
  /** Subtitle line for the step screen. */
  readonly subtitle: string;
  readonly presets: ReadonlyArray<{ id: string; label: string }>;
  /** Currently saved preset id for this step (defaults to 'default'). */
  activeId(prefs: UserThemePrefs): string;
}

export const wizardSteps: readonly WizardStep[] = [
  {
    id: 'prompt',
    label: 'prompt',
    subtitle: 'Select a prompt style',
    presets: promptPresets,
    activeId: (p) => p.promptPreset ?? 'default',
  },
  {
    id: 'response',
    label: 'response',
    subtitle: 'Select a response style',
    presets: responsePresets,
    activeId: (p) => p.responsePreset ?? 'default',
  },
  {
    id: 'diff',
    label: 'diff',
    subtitle: 'Select code diff colors',
    presets: diffPresets,
    activeId: (p) => p.diffPreset ?? 'default',
  },
];

export function getStep(id: WizardStepId): WizardStep {
  const step = wizardSteps.find((s) => s.id === id);
  if (!step) {
    throw new Error(`Unknown wizard step: ${id}`);
  }
  return step;
}

/** ID of the next wizard step, or null if `id` is the terminal step. */
export function nextStep(id: WizardStepId): WizardStepId | null {
  const idx = wizardSteps.findIndex((s) => s.id === id);
  return wizardSteps[idx + 1]?.id ?? null;
}

/** ID of the previous wizard step, or null when `id` is the first step. */
export function prevStep(id: WizardStepId): WizardStepId | null {
  const idx = wizardSteps.findIndex((s) => s.id === id);
  return idx > 0 ? wizardSteps[idx - 1]!.id : null;
}

// ─── Apply actions ───────────────────────────────────────────────────
//
// These are pure with respect to the file system — they save to disk
// and return the new prefs / colour deltas. The Panel passes the deltas
// to the runtime colour setters (setUserColors / setBaseTheme).

export interface ColorDelta {
  /** Pass null to clear, undefined to leave unchanged. Mirrors setUserColors. */
  prompt?: { text: any; bg: any } | null;
  response?: any | null;
  diff?: DiffPreset | null;
  /** Base theme: kiroDark/kiroLight, or null to reset to auto-detect. */
  baseTheme?: typeof kiroDark | typeof kiroLight | null;
}

export interface ApplyResult {
  /** New prefs after the action — pass into next render. */
  prefs: UserThemePrefs;
  /** Colour changes to push into the runtime theme. */
  delta: ColorDelta;
  /** Whether `saveUserThemePrefs` succeeded. */
  saved: boolean;
  /** Transient confirmation message for the runtime. */
  message: string;
}

/** Apply Auto: clear every preset and base-theme override. */
export function applyAuto(): ApplyResult {
  const prefs: UserThemePrefs = {};
  const saved = saveUserThemePrefs(prefs);
  return {
    prefs,
    delta: { prompt: null, response: null, diff: null, baseTheme: null },
    saved,
    message: saved
      ? 'Theme reset to default'
      : 'Theme reset but failed to save',
  };
}

/** Apply a bundled theme (Dark / Light). Returns null on unknown id. */
export function applyBundled(themeId: 'dark' | 'light'): ApplyResult | null {
  const bundled = bundledThemes.find((t) => t.id === themeId);
  if (!bundled) return null;

  const prefs: UserThemePrefs = {
    promptPreset:
      bundled.prompt.id === 'default' ? undefined : bundled.prompt.id,
    responsePreset:
      bundled.response.id === 'default' ? undefined : bundled.response.id,
    diffPreset: bundled.diff.id === 'default' ? undefined : bundled.diff.id,
    baseTheme: themeId,
  };
  const saved = saveUserThemePrefs(prefs);

  return {
    prefs,
    delta: {
      prompt: { text: bundled.prompt.textColor, bg: bundled.prompt.bgColor },
      response: bundled.response.textColor,
      diff: bundled.diff,
      baseTheme: themeId === 'dark' ? kiroDark : kiroLight,
    },
    saved,
    message: saved
      ? `Theme set to ${bundled.label}`
      : `Theme applied but failed to save`,
  };
}

/**
 * Apply a wizard step's preset selection. Returns null when `presetId`
 * isn't recognised for the step — the caller can surface an error.
 */
export function applyWizardStep(
  step: WizardStepId,
  presetId: string,
  prefs: UserThemePrefs
): ApplyResult | null {
  const next: UserThemePrefs = { ...prefs };
  let delta: ColorDelta;
  let presetLabel: string;

  if (step === 'prompt') {
    const preset = getPromptPreset(presetId);
    if (!preset) return null;
    next.promptPreset = preset.id === 'default' ? undefined : preset.id;
    delta = { prompt: { text: preset.textColor, bg: preset.bgColor } };
    presetLabel = preset.label;
  } else if (step === 'response') {
    const preset = getResponsePreset(presetId);
    if (!preset) return null;
    next.responsePreset = preset.id === 'default' ? undefined : preset.id;
    delta = { response: preset.textColor };
    presetLabel = preset.label;
  } else {
    const preset = getDiffPreset(presetId);
    if (!preset) return null;
    next.diffPreset = preset.id === 'default' ? undefined : preset.id;
    delta = { diff: preset };
    presetLabel = preset.label;
  }

  const saved = saveUserThemePrefs(next);
  // Mid-wizard saves bubble up via `saved`; terminal step uses a dedicated
  // confirmation message ("Theme updated. ✓") chosen by the panel.
  const message = saved
    ? `${presetLabel} applied`
    : `${presetLabel} applied but failed to save`;
  return { prefs: next, delta, saved, message };
}

// Re-export preset helpers so the panel can render previews from a single
// import surface.
export {
  getPromptPreset,
  getResponsePreset,
  getDiffPreset,
} from '../theme/user-theme.js';
export type { PromptPreset, ResponsePreset, DiffPreset, UserThemePrefs };
