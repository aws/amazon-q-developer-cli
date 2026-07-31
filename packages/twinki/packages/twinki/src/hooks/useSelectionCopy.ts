import { useEffect, useRef } from 'react';
import { useTwinkiContext } from './context.js';

export interface UseSelectionCopyOptions {
	isActive?: boolean;
}

/** Handles successful renderer text-selection copy attempts. */
export function useSelectionCopy(
	handler: (text: string) => void,
	options: UseSelectionCopyOptions = {},
): void {
	const { tui } = useTwinkiContext();
	const isActive = options.isActive ?? true;
	const handlerRef = useRef(handler);
	handlerRef.current = handler;

	useEffect(() => {
		if (!isActive) return;
		return tui.addSelectionCopyListener((text) => handlerRef.current(text));
	}, [tui, isActive]);
}
