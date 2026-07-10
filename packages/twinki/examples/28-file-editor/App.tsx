import React, { useCallback, useMemo, useState } from 'react';
import { basename } from 'node:path';
import { Box, useApp, useInput } from 'twinki';
import { EditorPane } from './components/EditorPane.js';
import { FileTree } from './components/FileTree.js';
import { StatusBar } from './components/StatusBar.js';
import { useEditorSession } from './hooks/useEditorSession.js';
import { useFileTree } from './hooks/useFileTree.js';
import { useThemeRotation } from './hooks/useThemeRotation.js';
import { useTerminalSize } from './hooks/useTerminalSize.js';
import { Workspace } from './lib/workspace.js';
import type { TreeNode } from './types.js';

const TREE_WIDTH = 32;

export interface AppProps {
	/** Absolute path of the folder to browse (scoped for all IO). */
	workspaceRoot: string;
}

/**
 * Composition root. Owns the one piece of cross-cutting state (editing =
 * which pane is focused), wires the three single-purpose hooks to the three
 * presentational panes, and routes keyboard input. Data flows top-down via
 * props; children report intent back up through callbacks.
 */
export const App: React.FC<AppProps> = ({ workspaceRoot }) => {
	const { exit } = useApp();
	const workspace = useMemo(() => new Workspace(workspaceRoot), [workspaceRoot]);

	const session = useEditorSession(workspace);
	const theme = useThemeRotation();
	const [editing, setEditing] = useState(false);

	// Fill the terminal: the panes take all rows except the 2-line statusline.
	const { rows: terminalRows } = useTerminalSize();
	const contentHeight = Math.max(3, terminalRows - 2);

	// Opening a file always lands in read-only view (NORMAL), Neovim-style.
	const { open: openSession } = session;
	const openFile = useCallback(
		(node: TreeNode) => {
			openSession(node);
			setEditing(false);
		},
		[openSession],
	);
	const tree = useFileTree(workspace, openFile);

	const { rows, selected, select, activate, moveUp, moveDown, toggle } = tree;
	const onRowClick = useCallback(
		(index: number) => {
			select(index);
			const row = rows[index];
			if (row) activate(row);
		},
		[rows, select, activate],
	);

	const { dirty, save } = session;
	const doSave = useCallback(() => {
		if (dirty) save();
	}, [dirty, save]);

	useInput((input, key) => {
		// Save works in both modes. Ctrl+S is not bound by the editor.
		if (key.ctrl && input === 's') {
			doSave();
			return;
		}

		// INSERT mode: the editor consumes everything except Esc (and Ctrl+S).
		if (editing) {
			if (key.escape) setEditing(false);
			return;
		}

		// NORMAL mode: drive the explorer.
		if (input === 'q') {
			exit();
			return;
		}
		if (key.tab && !key.shift) {
			theme.next();
			return;
		}
		if (key.tab && key.shift) {
			theme.prev();
			return;
		}
		if (key.upArrow || input === 'k') {
			moveUp();
			return;
		}
		if (key.downArrow || input === 'j') {
			moveDown();
			return;
		}

		const row = rows[selected];
		if (!row) return;
		if (key.leftArrow) {
			if (row.node.isDir && row.expanded) toggle(row.node);
			return;
		}
		if (key.rightArrow) {
			if (row.node.isDir && !row.expanded) toggle(row.node);
			return;
		}
		if (key.return) {
			activate(row);
			return;
		}
		if (input === 'e' && session.name) {
			setEditing(true);
			return;
		}
	});

	return (
		<Box flexDirection="column">
			<Box flexDirection="row" height={contentHeight}>
				<FileTree
					title={basename(workspace.root).toUpperCase()}
					rows={rows}
					selected={selected}
					active={!editing}
					width={TREE_WIDTH}
					onRowClick={onRowClick}
				/>
				<EditorPane
					path={session.path}
					relPath={session.relPath}
					language={session.language}
					theme={theme.theme}
					value={session.value}
					editing={editing}
					active={editing}
					paneHeight={contentHeight}
					onChange={session.change}
				/>
			</Box>
			<StatusBar
				editing={editing}
				fileName={session.name}
				language={session.language}
				dirty={session.dirty}
				theme={theme.theme}
				themeIndex={theme.index}
				themeCount={theme.count}
			/>
		</Box>
	);
};
