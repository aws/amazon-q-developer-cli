/**
 * Split — a two-pane layout divider for twinki (vertical or horizontal).
 *
 * Renders two children side-by-side (direction='row') or stacked
 * (direction='column') with a 1-cell separator, distributing space according
 * to `ratio` (0-1, proportion given to the first pane). Resizable via mouse
 * drag on the separator and keyboard (onResize callback).
 *
 * The active pane gets a highlighted border (visual focus indicator); which
 * pane is active is owned by the consumer (via `activePane`).
 */
import React, { useRef, useState } from 'react';
import { Box } from './Box.js';
import { Text } from './Text.js';
import { useMouse } from '../hooks/useMouse.js';

/** Props for the {@link Split} two-pane layout: direction, split `ratio`,
 * available size, which pane is active, and the drag/keyboard resize callback. */
export interface SplitProps {
	/** Which direction to split. 'row' = side-by-side; 'column' = stacked. */
	direction: 'row' | 'column';
	/** 0-1; proportion of available space given to pane A (first child). */
	ratio: number;
	/** Total available width in columns. */
	width: number;
	/** Total available height in rows. */
	height: number;
	/** Which pane is focused: 'a' (first) or 'b' (second). */
	activePane?: 'a' | 'b';
	/** Active pane border color. */
	activeColor?: string;
	/** Inactive pane border color. */
	inactiveColor?: string;
	/** Whether each pane renders its own rounded border. Defaults to true. */
	showPaneBorders?: boolean;
	/** Two children: pane A, pane B. */
	children: [React.ReactElement, React.ReactElement];
	/**
	 * Called with the new ratio while the separator is dragged. Omit to make
	 * the split fixed (keyboard-only resize stays with the consumer).
	 */
	onResize?: (ratio: number) => void;
}

const DEFAULT_ACTIVE = '#ffd866';
const DEFAULT_INACTIVE = '#727072';

/**
 * Two-pane split layout (row = side-by-side, column = stacked) with a 1-cell
 * separator. `ratio` (0-1) sizes pane A; `activePane` highlights the focused
 * pane's border. Children: exactly [paneA, paneB].
 */
export const Split: React.FC<SplitProps> = ({
	direction,
	ratio,
	width,
	height,
	activePane = 'a',
	activeColor = DEFAULT_ACTIVE,
	inactiveColor = DEFAULT_INACTIVE,
	showPaneBorders = true,
	children,
	onResize,
}) => {
	const [paneA, paneB] = children;

	// Drag-to-resize: ref tracks armed state (avoids a race where fast
	// press+release in one tick misses the disarming mouseup).
	const draggingRef = useRef(false);
	const [dragging, setDragging] = useState(false);
	const dragOrigin = useRef(0);
	const startDrag = (e: { x: number; y: number }) => {
		if (!onResize) return;
		const usable = Math.max(1, (direction === 'row' ? width : height) - 1);
		dragOrigin.current = (direction === 'row' ? e.x : e.y) - Math.max(1, Math.round(usable * ratio));
		draggingRef.current = true;
		setDragging(true);
	};
	useMouse((e) => {
		if (e.type === 'mouseup') {
			if (draggingRef.current) { draggingRef.current = false; setDragging(false); }
			return;
		}
		if (e.type !== 'mousemove' || e.button !== 'left' || !draggingRef.current || !onResize) return;
		const usable = Math.max(1, (direction === 'row' ? width : height) - 1);
		const aSize = (direction === 'row' ? e.x : e.y) - dragOrigin.current;
		onResize(Math.min(0.8, Math.max(0.2, aSize / usable)));
	});

	if (direction === 'row') {
		// Horizontal split: A | separator(1 col) | B
		const separatorWidth = 1;
		const usable = Math.max(0, width - separatorWidth);
		const aWidth = Math.max(1, Math.round(usable * ratio));
		const bWidth = Math.max(1, usable - aWidth);

		return (
			<Box flexDirection="row" width={width} height={height}>
				<Box
					width={aWidth}
					height={height}
					borderStyle={showPaneBorders ? 'round' : undefined}
					borderColor={activePane === 'a' ? activeColor : inactiveColor}
					flexDirection="column"
				>
					{paneA}
				</Box>
				<Box
					width={separatorWidth}
					height={height}
					flexDirection="column"
					justifyContent={showPaneBorders ? 'center' : undefined}
					onMouseDown={startDrag}
				>
					{showPaneBorders ? (
						<Text color={dragging ? activeColor : inactiveColor}>│</Text>
					) : (
						Array.from({ length: height }, (_, index) => (
							<Text key={index} color={dragging ? activeColor : inactiveColor}>
								│
							</Text>
						))
					)}
				</Box>
				<Box
					width={bWidth}
					height={height}
					borderStyle={showPaneBorders ? 'round' : undefined}
					borderColor={activePane === 'b' ? activeColor : inactiveColor}
					flexDirection="column"
				>
					{paneB}
				</Box>
			</Box>
		);
	}

	// Vertical split: A / separator(1 row) / B
	const separatorHeight = 1;
	const usable = Math.max(0, height - separatorHeight);
	const aHeight = Math.max(1, Math.round(usable * ratio));
	const bHeight = Math.max(1, usable - aHeight);

	return (
		<Box flexDirection="column" width={width} height={height}>
			<Box
				width={width}
				height={aHeight}
				borderStyle={showPaneBorders ? 'round' : undefined}
				borderColor={activePane === 'a' ? activeColor : inactiveColor}
				flexDirection="column"
			>
				{paneA}
			</Box>
			<Box width={width} height={separatorHeight} onMouseDown={startDrag}>
				<Text color={dragging ? activeColor : inactiveColor}>{'─'.repeat(width)}</Text>
			</Box>
			<Box
				width={width}
				height={bHeight}
				borderStyle={showPaneBorders ? 'round' : undefined}
				borderColor={activePane === 'b' ? activeColor : inactiveColor}
				flexDirection="column"
			>
				{paneB}
			</Box>
		</Box>
	);
};
