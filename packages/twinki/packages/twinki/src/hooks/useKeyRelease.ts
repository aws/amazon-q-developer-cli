import { useEffect, useRef } from 'react';
import { useTwinkiContext } from './context.js';
import { parseInputData } from './useInput.js';
import type { Key } from './useInput.js';

export interface UseKeyReleaseOptions {
	isActive?: boolean;
}

/**
 * Hook for handling key release events (Kitty keyboard protocol).
 * Only fires when the terminal supports key release reporting.
 */
export function useKeyRelease(
	handler: (input: string, key: Key) => void,
	options: UseKeyReleaseOptions = {},
): void {
	const { tui } = useTwinkiContext();
	const isActive = options.isActive ?? true;
	const handlerRef = useRef(handler);
	handlerRef.current = handler;

	useEffect(() => {
		if (!isActive) return;
		return tui.addKeyReleaseListener((data) => {
			const { input, key } = parseInputData(data);
			handlerRef.current(input, key);
		});
	}, [tui, isActive]);
}
