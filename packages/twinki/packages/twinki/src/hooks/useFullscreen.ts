import { useEffect } from 'react';
import { useTwinkiContext } from './context.js';

/**
 * Hook that enters alternate screen buffer on mount and exits on unmount.
 *
 * Prefer using `render(<App />, { fullscreen: true })` instead — that enters
 * alt screen before the first render, guaranteeing zero writes to the normal buffer.
 * This hook is for cases where fullscreen is toggled dynamically.
 */
export function useFullscreen(): void {
	const { tui } = useTwinkiContext();

	useEffect(() => {
		if (!tui.isAltScreen()) {
			tui.enterAltScreen();
		}
		return () => {
			if (tui.isAltScreen()) {
				tui.exitAltScreen();
			}
		};
	}, [tui]);
}
