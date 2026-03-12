import { useState, useCallback } from 'react';
import { useInput } from './useInput.js';

type UseScrollOptions = {
	isActive?: boolean;
	pageSize?: number;
};

type UseScrollResult = {
	scrollTop: number;
	scrollBy: (delta: number) => void;
	scrollTo: (offset: number) => void;
};

export const useScroll = (options: UseScrollOptions = {}): UseScrollResult => {
	const { isActive = true, pageSize = 10 } = options;
	const [scrollTop, setScrollTop] = useState(0);

	const scrollBy = useCallback((delta: number) => {
		setScrollTop(prev => Math.max(0, prev + delta));
	}, []);

	const scrollTo = useCallback((offset: number) => {
		setScrollTop(Math.max(0, offset));
	}, []);

	useInput(
		(_input, key) => {
			if (key.upArrow) scrollBy(-1);
			if (key.downArrow) scrollBy(1);
			if (key.pageUp) scrollBy(-pageSize);
			if (key.pageDown) scrollBy(pageSize);
		},
		{ isActive },
	);

	return { scrollTop, scrollBy, scrollTo };
};
