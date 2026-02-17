import React, {useEffect, useRef, useContext, useCallback, useMemo, type ReactNode} from 'react';
import Box from './Box.js';
import Text from './Text.js';
import Scrollbar from './Scrollbar.js';
import useScroll from '../hooks/use-scroll.js';
import StdinContext from './StdinContext.js';
import MouseContext from './MouseContext.js';
import StdoutContext from './StdoutContext.js';
import {type MouseEvent} from '../parse-mouse.js';

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
	const {scrollTop, scrollTo, scrollBy} = useScroll({isActive: true});
	const userScrolledRef = useRef(false);
	const prevScrollTopRef = useRef(0);

	// Mouse wheel support
	const {enableMouseTracking, disableMouseTracking} = useContext(MouseContext);
	const {internal_eventEmitter} = useContext(StdinContext);
	const {stdout} = useContext(StdoutContext);

	// Track whether mouse tracking is paused (scroll passed to terminal)
	const mousePassthroughRef = useRef(false);

	// Single effect for mouse tracking lifecycle — avoids cleanup ordering issues
	useEffect(() => {
		enableMouseTracking();
		return () => {
			// If passthrough was active, re-enable raw tracking before decrementing
			// so the ref-counted disable produces the correct final state.
			if (mousePassthroughRef.current) {
				stdout.write('\x1b[?1002;1006h');
				mousePassthroughRef.current = false;
			}
			disableMouseTracking();
		};
	}, [enableMouseTracking, disableMouseTracking, stdout]);

	const pauseMouseTracking = useCallback(() => {
		if (!mousePassthroughRef.current) {
			mousePassthroughRef.current = true;
			stdout.write('\x1b[?1002;1006l');
		}
	}, [stdout]);

	const resumeMouseTracking = useCallback(() => {
		if (mousePassthroughRef.current) {
			mousePassthroughRef.current = false;
			stdout.write('\x1b[?1002;1006h');
		}
	}, [stdout]);

	// Resume mouse tracking when keyboard scrolls down (arrow/pagedown still work via useInput in raw mode)
	useEffect(() => {
		if (scrollTop > 0 && mousePassthroughRef.current) {
			resumeMouseTracking();
		}
	}, [scrollTop, resumeMouseTracking]);

	const draggingRef = useRef(false);
	const scrollTopRef = useRef(scrollTop);
	scrollTopRef.current = scrollTop;

	const handleMouse = useCallback((event: MouseEvent) => {
		if (event.button === 'scrollUp') {
			if (scrollTopRef.current <= 0) {
				pauseMouseTracking();
				return;
			}
			scrollBy(-3);
			userScrolledRef.current = true;
		} else if (event.button === 'scrollDown') {
			resumeMouseTracking();
			scrollBy(3);
		} else if (event.button === 'left' && event.type === 'press') {
			draggingRef.current = true;
		} else if (event.type === 'release') {
			draggingRef.current = false;
		}

		// Drag on scrollbar: map row position to scroll offset
		if (draggingRef.current && (event.type === 'press' || event.type === 'drag')) {
			resumeMouseTracking();
			const ratio = Math.max(0, Math.min(1, (event.row - 1) / Math.max(1, height)));
			scrollTo(Math.round(ratio * maxScroll));
			userScrolledRef.current = true;
		}
	}, [scrollBy, scrollTo, height, maxScroll, pauseMouseTracking, resumeMouseTracking]);

	useEffect(() => {
		internal_eventEmitter?.on('mouse', handleMouse);
		return () => { internal_eventEmitter?.removeListener('mouse', handleMouse); };
	}, [internal_eventEmitter, handleMouse]);

	useEffect(() => {
		if (streaming) {
			if (!userScrolledRef.current) {
				resumeMouseTracking();
				scrollTo(maxScroll);
			}
		} else {
			// Streaming finished — restore passthrough state so ref count stays consistent
			resumeMouseTracking();
			userScrolledRef.current = false;
		}
	}, [streaming, maxScroll, scrollTo, resumeMouseTracking]);

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
				<Text dimColor italic>  PgUp/PgDn to scroll</Text>
			)}
		</Box>
	);
}
