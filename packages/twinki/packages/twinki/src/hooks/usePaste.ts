import { useEffect, useRef } from 'react';
import { useTwinkiContext } from './context.js';

export interface UsePasteOptions {
	isActive?: boolean;
}

/**
 * Hook for handling paste events (bracketed paste mode).
 * Receives the pasted text content without the bracketed paste markers.
 */
export function usePaste(
	handler: (content: string) => void,
	options: UsePasteOptions = {},
): void {
	const { tui } = useTwinkiContext();
	const isActive = options.isActive ?? true;
	const handlerRef = useRef(handler);
	handlerRef.current = handler;

	useEffect(() => {
		if (!isActive) return;
		return tui.addPasteListener((content) => handlerRef.current(content));
	}, [tui, isActive]);
}
