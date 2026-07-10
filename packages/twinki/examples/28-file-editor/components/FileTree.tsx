import React from 'react';
import { Box, Text } from 'twinki';
import { palette } from '../lib/palette.js';
import { TreeRow } from './TreeRow.js';
import type { VisibleRow } from '../types.js';

export interface FileTreeProps {
	/** Explorer header label (e.g. the workspace name). */
	title: string;
	rows: VisibleRow[];
	selected: number;
	/** Highlights the border when this pane holds focus. */
	active: boolean;
	/** Total pane width in columns. */
	width: number;
	onRowClick: (index: number) => void;
}

/**
 * Left pane: an nvim-tree-style file explorer. Presentational — it maps the
 * visible rows to TreeRow and forwards clicks up to the container.
 */
export const FileTree: React.FC<FileTreeProps> = ({ title, rows, selected, active, width, onRowClick }) => {
	const inner = width - 4; // account for border (2) + paddingX (2)
	return (
		<Box
			flexDirection="column"
			width={width}
			borderStyle="round"
			borderColor={active ? palette.yellow : palette.dim}
			paddingX={1}
		>
			<Text color={palette.green} bold>{title}</Text>
			{rows.map((row, i) => (
				<TreeRow
					key={row.node.path}
					row={row}
					selected={i === selected}
					width={inner}
					onActivate={() => onRowClick(i)}
				/>
			))}
		</Box>
	);
};
