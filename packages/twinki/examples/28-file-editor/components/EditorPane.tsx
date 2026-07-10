import React from 'react';
import { sep } from 'node:path';
import { Box, Text, EditorInput } from 'twinki';
import { palette } from '../lib/palette.js';
import { FileView } from './FileView.js';

export interface EditorPaneProps {
	/** Absolute path of the open file (also used as a remount key). */
	path: string | null;
	/** Path relative to the workspace root, for the breadcrumb. */
	relPath: string | null;
	/** shiki language id, or undefined for plain text. */
	language: string | undefined;
	/** Active shiki theme id. */
	theme: string;
	/** Buffer contents. */
	value: string;
	/** True when editing (INSERT); false is the read-only full-height view. */
	editing: boolean;
	/** Highlights the border when this pane holds focus. */
	active: boolean;
	/** Outer pane height in rows; used to fill the read-only view. */
	paneHeight: number;
	onChange: (value: string) => void;
}

/**
 * Right pane: a Neovim-style breadcrumb winbar over the file body.
 *   - View (NORMAL): a full-height, syntax-highlighted read-only FileView.
 *   - Edit (INSERT): the interactive EditorInput (remounted per file via key).
 */
export const EditorPane: React.FC<EditorPaneProps> = ({ path, relPath, language, theme, value, editing, active, paneHeight, onChange }) => {
	const breadcrumb = relPath ? relPath.split(sep).join(' › ') : 'No file open';
	const viewerHeight = Math.max(1, paneHeight - 3); // borders (2) + breadcrumb (1)
	return (
		<Box
			flexDirection="column"
			flexGrow={1}
			borderStyle="round"
			borderColor={active ? palette.yellow : palette.dim}
			paddingX={1}
		>
			<Text color={palette.purple} bold wrap="truncate">{breadcrumb}</Text>
			{path == null ? (
				<Text color={palette.dim}>Select a file in the explorer (↑/↓ then Enter, or click) to open it.</Text>
			) : editing ? (
				<EditorInput
					key={path}
					value={value}
					isActive
					onChange={onChange}
					syntaxHighlight={language}
					syntaxTheme={theme}
					lineNumbers
					disableSubmit
				/>
			) : (
				<FileView value={value} language={language} theme={theme} height={viewerHeight} />
			)}
		</Box>
	);
};
