import React, {useEffect, useRef, useMemo, type ReactNode} from 'react';
import Box from './Box.js';
import Text from './Text.js';
import Scrollbar from './Scrollbar.js';
import useScroll from '../hooks/use-scroll.js';
import {isMac} from '../utils/os.js';

export type Props = {
	readonly content: string;
	readonly streaming: boolean;
	readonly height: number;
	readonly scrollbar?: boolean;
	readonly scrollbarColor?: string;
	readonly children: (content: string, scrollInfo: {scrollTop: number; totalLines: number; isScrolledUp: boolean}) => ReactNode;
};

export default function StreamingPanel({content, streaming, height, scrollbar = true, scrollbarColor, children}: Props) {
	const lines = useMemo(() => content ? content.split('\n') : [], [content]);
	const totalLines = lines.length;
	// Always reserve 1 row for the hint line so layout never shifts
	const contentHeight = height - 1;
	const showScrollbar = scrollbar && totalLines > contentHeight;
	const maxScroll = Math.max(0, totalLines - contentHeight);
	const {scrollTop, scrollTo} = useScroll({isActive: true});
	const userScrolledRef = useRef(false);
	const prevScrollTopRef = useRef(0);

	useEffect(() => {
		if (streaming) {
			if (!userScrolledRef.current) {
				scrollTo(maxScroll);
			}
		} else {
			userScrolledRef.current = false;
		}
	}, [streaming, maxScroll, scrollTo]);

	useEffect(() => {
		if (streaming && scrollTop < prevScrollTopRef.current) {
			userScrolledRef.current = true;
		}
		if (streaming && scrollTop >= maxScroll) {
			userScrolledRef.current = false;
		}
		prevScrollTopRef.current = scrollTop;
	}, [streaming, scrollTop, maxScroll]);

	const clampedScroll = Math.min(scrollTop, maxScroll);
	const isScrolledUp = clampedScroll < maxScroll;
	const showIndicator = clampedScroll > 0;
	const effectiveHeight = showIndicator ? contentHeight - 1 : contentHeight;

	const visibleContent = useMemo(() => {
		if (totalLines <= contentHeight) return content;
		return lines.slice(clampedScroll, clampedScroll + effectiveHeight).join('\n');
	}, [lines, content, totalLines, contentHeight, clampedScroll, effectiveHeight]);

	return (
		<Box flexDirection="column" {...(showScrollbar ? {height} : {})}>
			<Box flexDirection="row" flexGrow={1} overflow="hidden">
				<Box width="98%" flexDirection="column">
					{showIndicator && (
						<Text dimColor>  ↑ {clampedScroll} lines above</Text>
					)}
					{children(visibleContent, {scrollTop: clampedScroll, totalLines, isScrolledUp})}
				</Box>
				{showScrollbar && (
					<Box width="2%">
						<Scrollbar
							scrollTop={clampedScroll}
							totalLines={totalLines}
							viewportHeight={contentHeight}
							color={scrollbarColor}
						/>
					</Box>
				)}
			</Box>
			{showScrollbar && (
				<Text dimColor italic>  {isMac ? 'Fn+↑/↓' : 'PgUp/PgDn'} to scroll</Text>
			)}
		</Box>
	);
}
