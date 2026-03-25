/**
 * Renderer proxy — re-exports from twinki or ink based on KIRO_RENDERER env var.
 * All components should import from here instead of directly from 'ink'.
 */

// Re-export types from ink (types are erased at runtime, no dual-instance issue)
export type { TextProps, BoxProps, Key } from 'ink';

const useTwinki = process.env.KIRO_RENDERER !== 'ink';

// Static imports so the bundler can resolve both packages.
// Only one is used at runtime based on KIRO_RENDERER.
import * as twinkiMod from 'twinki';
import * as inkMod from 'ink';

if (useTwinki && !twinkiMod?.render) {
  process.stderr.write(
    '[kiro-tui] FATAL: KIRO_RENDERER=twinki but twinki failed to load.\n'
  );
  process.exit(1);
}

const mod = useTwinki ? (twinkiMod as unknown as typeof inkMod) : inkMod;

export const Box = mod.Box as typeof import('ink').Box;
export const Text = mod.Text as typeof import('ink').Text;
export const Static = mod.Static as typeof import('ink').Static;
export const Newline = mod.Newline as typeof import('ink').Newline;
export const Spacer = mod.Spacer as typeof import('ink').Spacer;
export const Transform = mod.Transform as typeof import('ink').Transform;
export const useInput = mod.useInput as typeof import('ink').useInput;
export const useApp = mod.useApp as typeof import('ink').useApp;
export const useStdin = mod.useStdin as typeof import('ink').useStdin;
export const useStdout = mod.useStdout as typeof import('ink').useStdout;
export const useFocus = mod.useFocus as typeof import('ink').useFocus;
export const useFocusManager =
  mod.useFocusManager as typeof import('ink').useFocusManager;
export const render = mod.render as typeof import('ink').render;
// useMouse and measureElement are ink-specific; twinki has useMouse with a different signature
export const useMouse = (mod as any).useMouse;
export const measureElement = (mod as any).measureElement;
// usePaste: twinki-native hook, no-op shim under ink
export const usePaste: (
  handler: (content: string) => void,
  opts?: { isActive?: boolean }
) => void = useTwinki
  ? (mod as any).usePaste
  : (_handler: any, _opts?: any) => {
      void _handler;
      void _opts;
    };
// StreamingPanel: available in both ink and twinki
export const StreamingPanel =
  mod.StreamingPanel as typeof import('ink').StreamingPanel;
// CURSOR_MARKER: twinki uses this APC sequence to position the hardware cursor.
// Under ink it's empty (ink manages cursor separately).
export const CURSOR_MARKER: string = useTwinki
  ? (twinkiMod as any).CURSOR_MARKER
  : '';
