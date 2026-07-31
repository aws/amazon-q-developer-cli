import stripAnsi from 'strip-ansi';
import { sliceByColumn } from '../utils/slice.js';
import { getSegmenter, visibleWidth } from '../utils/visible-width.js';

export interface TextSelectionPoint {
	/** Zero-based logical line index. */
	row: number;
	/** Zero-based terminal column within the logical line. */
	column: number;
}

/** Rectangular selection boundary in logical render coordinates. */
export interface TextSelectionBounds {
	x: number;
	y: number;
	width: number;
	height: number;
}

interface OrderedSelection {
	start: TextSelectionPoint;
	end: TextSelectionPoint;
}

interface BoundsEdges {
	left: number;
	right: number;
	top: number;
	bottom: number;
}

const RESET = '\x1b[0m';
const INVERSE = '\x1b[7m';

function boundsEdges(bounds: TextSelectionBounds): BoundsEdges {
	const x = Math.floor(bounds.x);
	const y = Math.floor(bounds.y);
	return {
		left: Math.max(0, x),
		right: Math.max(0, Math.floor(x + bounds.width)),
		top: Math.max(0, y),
		bottom: Math.max(0, Math.floor(y + bounds.height)),
	};
}

/** Clamps a logical point to the cells inside a selection boundary. */
export function clampTextSelectionPoint(
	point: TextSelectionPoint,
	bounds: TextSelectionBounds,
): TextSelectionPoint {
	const edges = boundsEdges(bounds);
	return {
		row: Math.min(
			Math.max(point.row, edges.top),
			Math.max(edges.top, edges.bottom - 1),
		),
		column: Math.min(
			Math.max(point.column, edges.left),
			Math.max(edges.left, edges.right - 1),
		),
	};
}

function comparePoints(a: TextSelectionPoint, b: TextSelectionPoint): number {
	return a.row === b.row ? a.column - b.column : a.row - b.row;
}

function orderSelection(
	anchor: TextSelectionPoint,
	focus: TextSelectionPoint,
): OrderedSelection {
	return comparePoints(anchor, focus) <= 0
		? { start: anchor, end: focus }
		: { start: focus, end: anchor };
}

function graphemeBoundsAt(
	line: string,
	column: number,
): { start: number; end: number } {
	const text = stripAnsi(line);
	const target = Math.max(0, column);
	let currentColumn = 0;

	for (const { segment } of getSegmenter().segment(text)) {
		const width = visibleWidth(segment);
		if (width === 0) continue;

		const nextColumn = currentColumn + width;
		if (target < nextColumn) {
			return { start: currentColumn, end: nextColumn };
		}
		currentColumn = nextColumn;
	}

	return { start: currentColumn, end: currentColumn };
}

function selectedColumns(
	line: string,
	row: number,
	selection: OrderedSelection,
	bounds?: TextSelectionBounds,
): { start: number; end: number } {
	const lineWidth = visibleWidth(line);
	const edges = bounds ? boundsEdges(bounds) : null;
	const left = Math.min(edges?.left ?? 0, lineWidth);
	const right = Math.min(edges?.right ?? lineWidth, lineWidth);
	const start =
		row === selection.start.row
			? graphemeBoundsAt(line, selection.start.column).start
			: left;
	const end =
		row === selection.end.row
			? graphemeBoundsAt(line, selection.end.column).end
			: right;

	return {
		start: Math.min(Math.max(start, left), right),
		end: Math.min(Math.max(end, left), right),
	};
}

function selectedRows(
	lines: readonly string[],
	selection: OrderedSelection,
	bounds?: TextSelectionBounds,
): { first: number; last: number } {
	const edges = bounds ? boundsEdges(bounds) : null;
	return {
		first: Math.max(0, edges?.top ?? 0, selection.start.row),
		last: Math.min(
			lines.length - 1,
			(edges?.bottom ?? lines.length) - 1,
			selection.end.row,
		),
	};
}

/**
 * Extracts a terminal selection as plain text.
 *
 * Mouse endpoints are inclusive and snap to whole grapheme clusters. ANSI is
 * removed, line breaks are retained, and only terminal padding on the right
 * of each selected line is trimmed. When provided, `bounds` crops every row.
 */
export function extractSelectedText(
	lines: readonly string[],
	anchor: TextSelectionPoint,
	focus: TextSelectionPoint,
	bounds?: TextSelectionBounds,
): string {
	if (lines.length === 0) return '';

	const selection = orderSelection(anchor, focus);
	const { first, last } = selectedRows(lines, selection, bounds);
	if (first > last) return '';

	const selectedLines: string[] = [];
	for (let row = first; row <= last; row++) {
		const line = lines[row] ?? '';
		const columns = selectedColumns(line, row, selection, bounds);
		const selected = sliceByColumn(
			line,
			columns.start,
			columns.end - columns.start,
			true,
		);
		selectedLines.push(stripAnsi(selected).replace(/[ \t]+$/u, ''));
	}

	return selectedLines.join('\n');
}

/** Applies inverse-video highlighting without changing terminal cell widths. */
export function highlightTextSelection(
	lines: readonly string[],
	anchor: TextSelectionPoint,
	focus: TextSelectionPoint,
	bounds?: TextSelectionBounds,
): string[] {
	const result = [...lines];
	if (lines.length === 0) return result;

	const selection = orderSelection(anchor, focus);
	const { first, last } = selectedRows(lines, selection, bounds);

	for (let row = first; row <= last; row++) {
		const line = lines[row] ?? '';
		const lineWidth = visibleWidth(line);
		const columns = selectedColumns(line, row, selection, bounds);
		if (columns.end <= columns.start) continue;

		const before = sliceByColumn(line, 0, columns.start, true);
		const selected = stripAnsi(
			sliceByColumn(
				line,
				columns.start,
				columns.end - columns.start,
				true,
			),
		);
		const after = sliceByColumn(
			line,
			columns.end,
			lineWidth - columns.end,
			true,
		);

		result[row] = before + RESET + INVERSE + selected + RESET + after;
	}

	return result;
}
