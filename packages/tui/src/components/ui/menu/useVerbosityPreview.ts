import { useEffect, useState } from 'react';
import { useKeypress } from '../../../hooks/useKeypress.js';
import type { ActiveCommand } from '../../../stores/app-store';
import { DENSITY_PRESETS, type DensityPreset } from '../../../lite/verbose.js';

// Ctrl+P is the master switch (hidden ↔ mini); `p` refines (mini → expanded).
// Two keys so a stray `p` while typing can't pop a preview.
export type PreviewMode = 'mini' | 'expanded' | 'hidden';

/**
 * /verbosity preview lifecycle owned by CommandMenu's activeCommand branch:
 * the show/expand state machine, the highlighted-density draft, and the arming
 * keymap. Self-contained so the generic command menu stays free of lite-only
 * preview plumbing.
 */
export function useVerbosityPreview(activeCommand: ActiveCommand | null) {
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
  // the expanded-collapse live in CommandMenu's shared handler.)
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

  // /verbosity density rows: track the highlighted preset so the inline
  // preview can draft-render it; non-preset rows (Custom / back / Cancel)
  // clear the draft so the preview reverts to the saved config.
  const handleHighlight = (item: { label: string }) => {
    if (!activeCommand || activeCommand.command.name !== '/verbosity') return;
    const opt = activeCommand.options.find((o) => o.label === item.label);
    if (!opt) return;
    const m =
      opt.value.match(/^menu:density:confirm:([a-z]+)$/) ??
      opt.value.match(/^density:apply:([a-z]+)$/);
    setDraftPreset(
      m && DENSITY_PRESETS.includes(m[1] as DensityPreset)
        ? (m[1] as DensityPreset)
        : null
    );
  };

  return {
    previewMode,
    setPreviewMode,
    draftPreset,
    handleHighlight,
  };
}
