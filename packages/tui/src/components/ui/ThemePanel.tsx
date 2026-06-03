/**
 * /settings → theme overlay.
 *
 * Built from `<Panel>` (the chrome — title, dividers, footer), `<Menu>`
 * (the selectable list — owns ↑↓/Enter and the `❯` cursor) and an
 * inline preview block. The wizard step machine + apply actions live
 * in `utils/theme-wizard.ts` so they're testable without mounting the
 * panel.
 *
 * Why not Explorer? Theme's preview is a "what does this theme look
 * like" sample (coloured ▌ rows + diff block), not the
 * dialog-snippet preview pane Explorer ships for /rewind. The two
 * previews have different semantics; reusing Explorer here forced
 * theme content through the wrong renderer (everything dimmed,
 * entire body wrapped in a single ▌). Composing Panel + Menu lets
 * us own the preview block while still sharing the overlay frame
 * with /rewind and /settings.
 *
 * Two screens:
 *   1. Top-level — Auto / Dark theme / Light theme / Custom.
 *   2. Wizard step — prompt → response → diff (only via Custom).
 *
 * Auto / Dark / Light apply immediately and close. Custom advances
 * through the wizard, persisting on Enter and confirming with
 * "Theme updated. ✓" when the diff step is selected. ESC walks back
 * one level (wizard step → previous step → top-level → close).
 */

import React, { useCallback, useMemo, useState } from 'react';
import { Box } from './../../renderer.js';
import { Panel } from './panel/Panel.js';
import { Text } from './text/Text.js';
import { Menu, type MenuItem } from './menu/Menu.js';
import { useTheme } from '../../hooks/useThemeContext.js';
import { useAppStore } from '../../stores/app-store.js';
import {
  bundledThemes,
  buildBundledPreview,
  buildCurrentPreview,
  buildFallbackDiff,
  loadUserThemePrefs,
  type DiffPreset,
  type PromptPreset,
  type ResponsePreset,
  type UserThemePrefs,
  getPromptPreset,
  getResponsePreset,
  getDiffPreset,
  promptPresets,
  responsePresets,
  diffPresets,
} from '../../theme/user-theme.js';
import {
  applyAuto,
  applyBundled,
  applyWizardStep,
  buildTopLevelItems,
  getStep,
  nextStep,
  prevStep,
  wizardSteps,
  type ColorDelta,
  type TopLevelChoice,
  type WizardStepId,
} from '../../utils/theme-wizard.js';

interface ThemePanelProps {
  onClose: () => void;
}

type Screen = { type: 'top-level' } | { type: 'wizard'; step: WizardStepId };

export const ThemePanel: React.FC<ThemePanelProps> = ({ onClose }) => {
  const { setUserColors, setBaseTheme, baseTheme, getColor } = useTheme();
  const dim = getColor('secondary');
  const primary = getColor('primary');
  const showAlert = useAppStore((state) => state.showTransientAlert);
  const autoPreviewGetter = useAppStore((state) => state._autoPreviewGetter);
  // Read once at mount: the flag is set by /settings before this panel
  // opens. Drives the footer hint label ('to go back' vs 'to cancel');
  // back-navigation itself happens via `onClose` upstream.
  const fromSettings = useAppStore((state) => state.settingsReturnOnEscape);
  // Direct store actions for the final-confirm path: we close the
  // panel ourselves and clear the back-flag, bypassing onClose's
  // settings-reopen branch — per UX spec, "once the user reaches the
  // last layer and confirms a choice, we close the entire menu". ESC
  // mid-flow still walks back one level via onClose.
  const setShowThemePanel = useAppStore((state) => state.setShowThemePanel);
  const setSettingsReturnOnEscape = useAppStore(
    (state) => state.setSettingsReturnOnEscape
  );

  const [screen, setScreen] = useState<Screen>({ type: 'top-level' });
  const [prefs, setPrefs] = useState<UserThemePrefs>(() =>
    loadUserThemePrefs()
  );
  // Mirror Menu's selected index here for preview rendering. Menu owns
  // the actual cursor (we drive it via key-based remount on screen
  // changes).
  const [highlightIndex, setHighlightIndex] = useState(0);

  const screenKey =
    screen.type === 'top-level' ? 'top-level' : `wizard:${screen.step}`;

  const fallbackDiff = useMemo(
    () =>
      buildFallbackDiff({
        added: {
          background: baseTheme.colors.diff.added.background,
          bar: baseTheme.colors.diff.added.bar,
          highlight: baseTheme.colors.diff.added.highlight,
        },
        removed: {
          background: baseTheme.colors.diff.removed.background,
          bar: baseTheme.colors.diff.removed.bar,
          highlight: baseTheme.colors.diff.removed.highlight,
        },
      }),
    [baseTheme]
  );

  // Push a colour delta into the runtime theme. `setUserColors` accepts
  // `undefined` (leave alone) and `null` (clear) per slot;
  // `setBaseTheme(null)` resets to auto-detect.
  const pushDelta = useCallback(
    (delta: ColorDelta) => {
      if ('baseTheme' in delta) {
        setBaseTheme(delta.baseTheme ?? null);
      }
      setUserColors(delta.prompt, delta.response, delta.diff);
    },
    [setBaseTheme, setUserColors]
  );

  // ─── Action handlers ────────────────────────────────────────────
  const handleTopLevelSelect = useCallback(
    (choice: TopLevelChoice) => {
      if (choice === 'custom') {
        setScreen({ type: 'wizard', step: wizardSteps[0]!.id });
        return;
      }

      const result =
        choice === 'auto'
          ? applyAuto()
          : applyBundled(choice === 'bundled-dark' ? 'dark' : 'light');
      if (!result) return;

      pushDelta(result.delta);
      setPrefs(result.prefs);
      showAlert({
        message: result.message,
        status: result.saved ? 'success' : 'error',
        autoHideMs: 3000,
      });
      // Final apply: close the overlay directly. Bypasses onClose so
      // we don't trigger the settings-reopen branch in
      // handleCloseThemePanel.
      setSettingsReturnOnEscape(false);
      setShowThemePanel(false);
    },
    [pushDelta, showAlert, setSettingsReturnOnEscape, setShowThemePanel]
  );

  const handleWizardSelect = useCallback(
    (stepId: WizardStepId, presetId: string) => {
      const result = applyWizardStep(stepId, presetId, prefs);
      if (!result) {
        showAlert({
          message: `Unknown ${getStep(stepId).label} preset: ${presetId}`,
          status: 'error',
          autoHideMs: 3000,
        });
        return;
      }

      pushDelta(result.delta);
      setPrefs(result.prefs);

      const next = nextStep(stepId);
      if (next) {
        if (!result.saved) {
          showAlert({
            message: result.message,
            status: 'error',
            autoHideMs: 3000,
          });
        }
        setScreen({ type: 'wizard', step: next });
      } else {
        showAlert({
          message: result.saved
            ? 'Theme updated. ✓'
            : 'Theme applied but failed to save',
          status: result.saved ? 'success' : 'error',
          autoHideMs: 3000,
        });
        // Final wizard step: close the overlay directly. See note in
        // handleTopLevelSelect for why we bypass onClose.
        setSettingsReturnOnEscape(false);
        setShowThemePanel(false);
      }
    },
    [prefs, pushDelta, showAlert, setSettingsReturnOnEscape, setShowThemePanel]
  );

  // ─── Items ──────────────────────────────────────────────────────
  // Each row is a MenuItem (shape Menu wants) plus an `onSelect` for
  // dispatch. We look up the row by label in handleSelect/handleHighlight
  // because that's the field Menu echoes back.
  interface Row extends MenuItem {
    onSelect: () => void;
  }

  const rows: Row[] = useMemo(() => {
    if (screen.type === 'top-level') {
      return buildTopLevelItems(prefs).map((item) => ({
        label: item.label,
        description: item.description,
        onSelect: () => handleTopLevelSelect(item.choice),
      }));
    }
    const step = getStep(screen.step);
    const activeId = step.activeId(prefs);
    return step.presets.map((p) => ({
      label: p.label,
      description: p.id === activeId ? '[active]' : '',
      onSelect: () => handleWizardSelect(step.id, p.id),
    }));
  }, [screen, prefs, handleTopLevelSelect, handleWizardSelect]);

  const handleSelect = useCallback(
    (item: MenuItem) => {
      const row = rows.find((r) => r.label === item.label);
      row?.onSelect();
    },
    [rows]
  );

  const handleHighlight = useCallback(
    (item: MenuItem) => {
      const idx = rows.findIndex((r) => r.label === item.label);
      if (idx >= 0) setHighlightIndex(idx);
    },
    [rows]
  );

  // ─── Back-navigation ────────────────────────────────────────────
  // Panel reads its closeMenu binding via useInput and calls onClose;
  // we route that through `handleBack` so ESC inside a wizard step
  // returns to the previous step (and the top-level screen) before
  // dismissing the overlay.
  const handleBack = useCallback(() => {
    if (screen.type === 'top-level') {
      onClose();
      return;
    }
    const previous = prevStep(screen.step);
    setScreen(
      previous ? { type: 'wizard', step: previous } : { type: 'top-level' }
    );
  }, [screen, onClose]);

  // ─── Preview ────────────────────────────────────────────────────
  const previewBody = useMemo(
    () =>
      buildPreviewForRow({
        screen,
        prefs,
        highlightIndex,
        fallbackDiff,
        autoPreview: autoPreviewGetter?.() ?? '',
        brandColor: baseTheme.colors.brand,
      }),
    [screen, prefs, highlightIndex, fallbackDiff, autoPreviewGetter, baseTheme]
  );

  // ─── Render ─────────────────────────────────────────────────────
  const subtitle =
    screen.type === 'top-level'
      ? 'Select the theme that looks best for your terminal'
      : getStep(screen.step).subtitle;
  const title =
    screen.type === 'top-level'
      ? '/settings – theme'
      : '/settings – theme – custom';
  const closeHintLabel =
    screen.type === 'wizard' || fromSettings ? 'to go back' : 'to cancel';
  // The diff step is the user's final confirmation — Enter applies
  // the theme and closes the overlay rather than advancing. Other
  // screens are pure selection (Auto/Dark/Light/Custom rows, or the
  // colour pickers in earlier wizard steps), so 'to select' fits.
  const isFinalStep = screen.type === 'wizard' && screen.step === 'diff';
  const enterLabel = isFinalStep ? 'to apply and close' : 'to select';

  const menuItems: MenuItem[] = rows.map((r) => ({
    label: r.label,
    description: r.description,
  }));

  return (
    <Panel
      title={title}
      onClose={handleBack}
      closeHintLabel={closeHintLabel}
      footerLeft={
        <Text>
          {primary('↑↓')} {dim('to navigate')}
          {dim(' · ')}
          {primary('↵')} {dim(enterLabel)}
        </Text>
      }
    >
      <Box height={1} />
      <Box paddingX={1} marginBottom={1}>
        <Text>{dim(subtitle)}</Text>
      </Box>

      {/* `key` forces Menu to remount and reset its internal cursor on
          screen changes — Menu only resets selection when its search
          text changes, not on items prop changes. */}
      <Menu
        key={screenKey}
        items={menuItems}
        prefix=""
        onSelect={handleSelect}
        onHighlight={handleHighlight}
        showSelectedIndicator
        showFooterHints={false}
      />

      <PreviewBlock body={previewBody} dim={dim} />
    </Panel>
  );
};

// ─── Subcomponents ────────────────────────────────────────────────

interface PreviewBlockProps {
  body: string;
  dim: (s: string) => string;
}

/**
 * Spec preview block: dim "──preview──" header, blank line, then the
 * pre-styled chalk body (prompt/response rows + diff block) emitted
 * from `buildBundledPreview` / `buildCurrentPreview` / the registered
 * `_autoPreviewGetter`. The body already carries its own ANSI styling,
 * so we render it as-is — no wrapping `Text` colour override.
 */
const PreviewBlock: React.FC<PreviewBlockProps> = ({ body, dim }) => {
  // Match the spec's `──preview────────...` divider — short ASCII run
  // with the word inline.
  const header = dim('──preview' + '─'.repeat(54));
  return (
    <Box marginTop={1} paddingX={1} flexDirection="column">
      <Text>{header}</Text>
      <Box height={1} />
      <Text>{body}</Text>
    </Box>
  );
};

// ─── Preview composition ──────────────────────────────────────────

interface BuildPreviewArgs {
  screen: Screen;
  prefs: UserThemePrefs;
  highlightIndex: number;
  fallbackDiff: DiffPreset;
  /** Pre-rendered "Auto" preview from the registered theme bridge. */
  autoPreview: string;
  /** Active theme's brand colour for the ▌ bar. */
  brandColor: import('../../types/themeTypes.js').TerminalColor;
}

/**
 * Produce the styled preview body for the highlighted row.
 *
 * - Top-level Auto highlight → use the registered auto preview (the
 *   terminal's natural colors, no overrides).
 * - Top-level Dark/Light highlight → sample the corresponding bundled
 *   theme.
 * - Top-level Custom highlight → show the user's currently saved combo.
 * - Wizard step highlight → take the highlighted preset and combine it
 *   with the saved-preset values for the other two slots so earlier
 *   choices stay locked in (per spec).
 */
function buildPreviewForRow({
  screen,
  prefs,
  highlightIndex,
  fallbackDiff,
  autoPreview,
  brandColor,
}: BuildPreviewArgs): string {
  if (screen.type === 'top-level') {
    // Order matches buildTopLevelItems: 0 Auto, 1 Dark, 2 Light, 3 Custom.
    if (highlightIndex === 0) return autoPreview;
    if (highlightIndex === 1 || highlightIndex === 2) {
      const id = highlightIndex === 1 ? 'dark' : 'light';
      const bundled = bundledThemes.find((t) => t.id === id);
      return bundled
        ? buildBundledPreview(bundled, fallbackDiff, brandColor)
        : '';
    }
    return buildCurrentPreview(prefs, fallbackDiff, brandColor);
  }

  // Wizard step — combine the highlighted preset with the saved values
  // for the other two slots.
  const step = getStep(screen.step);
  const highlighted = step.presets[highlightIndex];
  if (!highlighted) return '';

  const promptId =
    step.id === 'prompt' ? highlighted.id : (prefs.promptPreset ?? 'default');
  const responseId =
    step.id === 'response'
      ? highlighted.id
      : (prefs.responsePreset ?? 'default');
  const diffId =
    step.id === 'diff' ? highlighted.id : (prefs.diffPreset ?? 'default');

  const prompt: PromptPreset = getPromptPreset(promptId) ?? promptPresets[0]!;
  const response: ResponsePreset =
    getResponsePreset(responseId) ?? responsePresets[0]!;
  const diff: DiffPreset = getDiffPreset(diffId) ?? diffPresets[0]!;

  return buildBundledPreview(
    { id: 'preview', label: 'Preview', prompt, response, diff },
    fallbackDiff,
    brandColor
  );
}
