import { useEffect, useRef } from 'react';
import { useTwinkiContext } from './context.js';
import type { MouseEvent } from '../input/mouse.js';

export type { MouseEvent } from '../input/mouse.js';

export interface UseMouseOptions {
	isActive?: boolean;
}

/**
 * Hook for handling mouse events in Twinki applications.
 * Automatically enables/disables SGR mouse tracking.
 */
export function useMouse(
	handler: (event: MouseEvent) => void,
	options: UseMouseOptions = {},
): void {
	const { tui } = useTwinkiContext();
	const isActive = options.isActive ?? true;
	const handlerRef = useRef(handler);
	handlerRef.current = handler;

	useEffect(() => {
		if (!isActive) return;
		return tui.addMouseListener((event) => handlerRef.current(event));
	}, [tui, isActive]);
}
