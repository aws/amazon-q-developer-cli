import React from 'react';
import { Box } from './Box.js';
import { Text } from './Text.js';

export type ScrollbarProps = {
	readonly scrollTop: number;
	readonly totalLines: number;
	readonly viewportHeight: number;
	readonly color?: string;
	/** Thumb (active) color; defaults to `color`. */
	readonly thumbColor?: string;
	/**
	 * Clickable (ratatui-style): fires with a target scrollTop when the user
	 * clicks the track/arrows. ▲/▼ step by one; clicking the track pages toward
	 * the click; clicking the thumb region is a no-op. Omit for a static bar.
	 */
	readonly onScrollTo?: (scrollTop: number) => void;
};

/**
 * Vertical scrollbar: ▲ up-arrow, a proportional thumb over a ░ track, ▼
 * down-arrow. When `onScrollTo` is supplied each cell is individually
 * clickable (arrows step ±1, track pages), giving a mouse-draggable feel
 * without a drag protocol — the ratatui `Scrollbar` interaction model.
 */
export function Scrollbar({
	scrollTop,
	totalLines,
	viewportHeight,
	color,
	thumbColor,
	onScrollTo,
}: ScrollbarProps): React.ReactElement | null {
	if (totalLines <= viewportHeight) return null;

	const trackHeight = Math.max(1, viewportHeight - 2);
	const thumbSize = Math.max(1, Math.round((viewportHeight / totalLines) * trackHeight));
	const maxScroll = totalLines - viewportHeight;
	const scrollRatio = maxScroll > 0 ? scrollTop / maxScroll : 0;
	const thumbPos = Math.round(scrollRatio * (trackHeight - thumbSize));
	const trackColor = color ?? 'gray';
	const thumb = thumbColor ?? color ?? 'gray';

	// Static (non-interactive) fast path — one Text block, unchanged behavior.
	if (!onScrollTo) {
		const track = '▲\n' + Array.from({ length: trackHeight }, (_, i) =>
			i >= thumbPos && i < thumbPos + thumbSize ? '█' : '░',
		).join('\n') + '\n▼';
		return (
			<Box flexShrink={0} width={1}>
				<Text color={trackColor}>{track}</Text>
			</Box>
		);
	}

	// Clickable: each cell is its own Box with an onClick.
	const clamp = (n: number) => Math.max(0, Math.min(maxScroll, n));
	return (
		<Box flexDirection="column" flexShrink={0} width={1}>
			<Box onClick={() => onScrollTo(clamp(scrollTop - 1))}>
				<Text color={trackColor}>▲</Text>
			</Box>
			{Array.from({ length: trackHeight }, (_, i) => {
				const onThumb = i >= thumbPos && i < thumbPos + thumbSize;
				// Track click jumps to the scrollTop that positions the thumb at
				// the clicked row (ratatui: click-to-position); clicking the thumb
				// itself is a no-op.
				const target = clamp(Math.round((i / Math.max(1, trackHeight - 1)) * maxScroll));
				return (
					<Box key={i} onClick={() => onScrollTo(onThumb ? scrollTop : target)}>
						<Text color={onThumb ? thumb : trackColor}>{onThumb ? '█' : '░'}</Text>
					</Box>
				);
			})}
			<Box onClick={() => onScrollTo(clamp(scrollTop + 1))}>
				<Text color={trackColor}>▼</Text>
			</Box>
		</Box>
	);
}
