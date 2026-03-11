import { useEffect, useRef } from 'react';
import { useTwinkiContext } from './context.js';
import { parseInputData } from './useInput.js';
import type { Key } from './useInput.js';

export interface UseKeyRepeatOptions {
	isActive?: boolean;
}

/**
 * Hook for handling key repeat events (Kitty keyboard protocol).
 * Only fires when the terminal supports key repeat reporting.
 */
export function useKeyRepeat(
	handler: (input: string, key: Key) => void,
	options: UseKeyRepeatOptions = {},
): void {
	const { tui } = useTwinkiContext();
	const isActive = options.isActive ?? true;
	const handlerRef = useRef(handler);
	handlerRef.current = handler;

	useEffect(() => {
		if (!isActive) return;
		return tui.addKeyRepeatListener((data) => {
			const { input, key } = parseInputData(data);
			handlerRef.current(input, key);
		});
	}, [tui, isActive]);
}
