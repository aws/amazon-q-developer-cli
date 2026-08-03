import { useCallback, useContext, useSyncExternalStore } from 'react';
import { TwinkiCtx } from './context.js';
import { isHardwareCursorEnabled } from '../renderer/hardware-cursor.js';

/**
 * Whether the terminal's own cursor is visible, tracking the renderer's live
 * state rather than re-deriving it.
 *
 * A cursor drawn by inverting a cell is invisible if the terminal also parks
 * its own cursor there, so a component that paints one has to agree with the
 * renderer. Reading the environment separately would disagree the moment a
 * caller overrides the renderer's default or changes it later.
 *
 * Outside a render tree there is no renderer to ask, so the environment
 * default applies -- the same value a renderer would have started from.
 */
export function useHardwareCursor(): boolean {
	const tui = useContext(TwinkiCtx)?.tui;
	const subscribe = useCallback(
		(onChange: () => void) =>
			tui ? tui.onHardwareCursorChange(onChange) : () => {},
		[tui],
	);
	const getSnapshot = useCallback(
		() => (tui ? tui.hardwareCursorVisible : isHardwareCursorEnabled()),
		[tui],
	);
	return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}
