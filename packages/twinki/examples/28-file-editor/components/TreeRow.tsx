import React from 'react';
import { Box, Text } from 'twinki';
import { palette } from '../lib/palette.js';
import type { VisibleRow } from '../types.js';

export interface TreeRowProps {
	row: VisibleRow;
	/** Whether this row is the cursor row. */
	selected: boolean;
	/** Inner content width, so the selection highlight fills the pane. */
	width: number;
	/** Click handler: select + open/toggle. */
	onActivate: () => void;
}

/**
 * A single nvim-tree-style row: indentation + a folder arrow (▸/▾) or file
 * marker (·) + the name. Purely presentational; all state arrives via props.
 */
export const TreeRow: React.FC<TreeRowProps> = ({ row, selected, width, onActivate }) => {
	const { node, depth, expanded } = row;
	const arrow = node.isDir ? (expanded ? '▾ ' : '▸ ') : '· ';
	const label = `${'  '.repeat(depth)}${arrow}${node.name}`;
	const padded = label.length > width ? label.slice(0, Math.max(0, width - 1)) + '…' : label.padEnd(width);
	const fg = selected ? palette.bg : node.isDir ? palette.blue : palette.fg;

	return (
		<Box onClick={onActivate}>
			<Text
				color={fg}
				backgroundColor={selected ? palette.yellow : undefined}
				bold={node.isDir}
				wrap="truncate"
			>
				{padded}
			</Text>
		</Box>
	);
};
