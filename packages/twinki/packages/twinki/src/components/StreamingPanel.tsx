import React, { useEffect, useRef, useLayoutEffect, useMemo, type ReactNode } from 'react';
import { Box } from './Box.js';
import { Text } from './Text.js';
import { Scrollbar } from './Scrollbar.js';
import { useScroll } from '../hooks/useScroll.js';
import { measureElement } from '../index.js';

const isMac = process.platform === 'darwin';

export type StreamingPanelProps = {
	readonly content: string;
	readonly streaming: boolean;
	readonly height: number;
	readonly scrollbar?: boolean;
	readonly scrollbarColor?: string;
	readonly onReadyToFlush?: () => void;
	readonly children: (content: string, scrollInfo: { scrollTop: number; totalLines: number; isScrolledUp: boolean }) => ReactNode;
};

export function StreamingPanel({ content, streaming, height, scrollbar = true, scrollbarColor, onReadyToFlush, children }: StreamingPanelProps): React.ReactElement {
	const lines = useMemo(() => content ? content.split('\n') : [], [content]);
	const totalLines = lines.length;
	const contentHeight = height - 1;
	const showScrollbar = scrollbar && totalLines > contentHeight;
	const maxScroll = Math.max(0, totalLines - contentHeight);
	const { scrollTop, scrollTo } = useScroll({ isActive: true });
	const userScrolledRef = useRef(false);
	const prevScrollTopRef = useRef(0);
	const wasScrollableRef = useRef(false);

	if (showScrollbar) wasScrollableRef.current = true;

	useEffect(() => {
		if (!streaming && wasScrollableRef.current && onReadyToFlush) {
			onReadyToFlush();
		}
	}, [streaming]);

	useEffect(() => {
		if (streaming) {
			if (!userScrolledRef.current) scrollTo(maxScroll);
		} else {
			userScrolledRef.current = false;
		}
	}, [streaming, maxScroll, scrollTo]);

	useEffect(() => {
		if (streaming && scrollTop < prevScrollTopRef.current) userScrolledRef.current = true;
		if (streaming && scrollTop >= maxScroll) userScrolledRef.current = false;
		prevScrollTopRef.current = scrollTop;
	}, [streaming, scrollTop, maxScroll]);

	const clampedScroll = Math.min(scrollTop, maxScroll);
	const isScrolledUp = clampedScroll < maxScroll;

	const contentRef = useRef<any>(null);
	const renderedHeightRef = useRef(0);

	// Measure synchronously after layout — useLayoutEffect fires before
	// paint so the capped scroll is always consistent within the same frame.
	useLayoutEffect(() => {
		if (contentRef.current) {
			const { height: h } = measureElement(contentRef.current);
			if (h > 0) renderedHeightRef.current = h;
		}
	});

	const renderedHeight = renderedHeightRef.current;

	if (!showScrollbar) {
		return (
			<Box flexDirection="column">
				<Box ref={contentRef}>
					{children(content, { scrollTop: 0, totalLines: totalLines, isScrolledUp: false })}
				</Box>
			</Box>
		);
	}

	// Cap scroll so marginTop never exceeds rendered content height
	const maxRenderedScroll = renderedHeight > contentHeight
		? renderedHeight - contentHeight
		: maxScroll;
	const effectiveScroll = Math.min(clampedScroll, maxRenderedScroll);
	const effectiveIsScrolledUp = effectiveScroll < maxRenderedScroll;

	// Remaining visual lines from scroll position
	const remaining = Math.max(1, renderedHeight > 0 ? renderedHeight - effectiveScroll : totalLines - effectiveScroll);
	const visibleHeight = Math.min(contentHeight, remaining);

	const hintText = !streaming
		? `  ✓ Streaming complete · ${totalLines} lines · ${isMac ? 'Fn+↑/↓' : 'PgUp/PgDn'} to scroll`
		: clampedScroll > 0
			? `  ↑ ${clampedScroll} lines above · ${isMac ? 'Fn+↑/↓' : 'PgUp/PgDn'} to scroll`
			: `  ${isMac ? 'Fn+↑/↓' : 'PgUp/PgDn'} to scroll`;

	return (
		<Box flexDirection="column">
			<Box height={visibleHeight} overflow="hidden" flexDirection="column">
				<Box flexDirection="row">
					<Box width="98%" flexDirection="column" marginTop={-effectiveScroll}>
						<Box ref={contentRef} flexDirection="column">
							{children(content, { scrollTop: effectiveScroll, totalLines: totalLines, isScrolledUp: effectiveIsScrolledUp })}
						</Box>
					</Box>
					<Box width="2%">
						<Scrollbar
							scrollTop={effectiveScroll}
							totalLines={renderedHeight > 0 ? renderedHeight : totalLines}
							viewportHeight={visibleHeight}
							color={scrollbarColor}
						/>
					</Box>
				</Box>
			</Box>
			<Text dimColor italic>{hintText}</Text>
		</Box>
	);
}
