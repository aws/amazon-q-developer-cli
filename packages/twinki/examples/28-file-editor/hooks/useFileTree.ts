import { useCallback, useEffect, useMemo, useState } from 'react';
import type { Workspace } from '../lib/workspace.js';
import type { TreeNode, VisibleRow } from '../types.js';

/** Depth-first flatten of the currently expanded nodes into a render list. */
function flatten(node: TreeNode, expanded: ReadonlySet<string>, depth: number, out: VisibleRow[]): void {
	for (const child of node.children ?? []) {
		const isOpen = child.isDir && expanded.has(child.path);
		out.push({ node: child, depth, expanded: isOpen });
		if (isOpen) flatten(child, expanded, depth + 1, out);
	}
}

export interface FileTreeModel {
	/** Flattened list of visible rows. */
	rows: VisibleRow[];
	/** Index of the selected row (always in range). */
	selected: number;
	moveUp: () => void;
	moveDown: () => void;
	/** Expand or collapse a directory node. */
	toggle: (node: TreeNode) => void;
	/** Activate a row: toggle a directory, or open a file. */
	activate: (row: VisibleRow) => void;
	/** Select a row by index (e.g. on click). */
	select: (index: number) => void;
}

/**
 * Tree navigation state: expand/collapse, cursor selection, and the flattened
 * list of visible rows. Pure UI logic — filesystem access lives in Workspace,
 * and opening a file is delegated upward via `onOpenFile`.
 */
export function useFileTree(workspace: Workspace, onOpenFile: (node: TreeNode) => void): FileTreeModel {
	const root = useMemo(() => workspace.readTree(), [workspace]);

	// Top-level directories start expanded so the explorer isn't empty on launch.
	const [expanded, setExpanded] = useState<Set<string>>(
		() => new Set((root.children ?? []).filter((n) => n.isDir).map((n) => n.path)),
	);
	const [selected, setSelected] = useState(0);

	const rows = useMemo(() => {
		const out: VisibleRow[] = [];
		flatten(root, expanded, 0, out);
		return out;
	}, [root, expanded]);

	// Keep the cursor in range when the visible row count shrinks (collapse).
	useEffect(() => {
		setSelected((i) => Math.min(i, Math.max(0, rows.length - 1)));
	}, [rows.length]);

	const toggle = useCallback((node: TreeNode) => {
		if (!node.isDir) return;
		setExpanded((prev) => {
			const next = new Set(prev);
			if (next.has(node.path)) next.delete(node.path);
			else next.add(node.path);
			return next;
		});
	}, []);

	const activate = useCallback(
		(row: VisibleRow) => {
			if (row.node.isDir) toggle(row.node);
			else onOpenFile(row.node);
		},
		[toggle, onOpenFile],
	);

	const moveUp = useCallback(() => setSelected((i) => Math.max(0, i - 1)), []);
	const moveDown = useCallback(() => setSelected((i) => Math.min(rows.length - 1, i + 1)), [rows.length]);
	const select = useCallback((index: number) => setSelected(index), []);

	const safeSelected = rows.length ? Math.min(selected, rows.length - 1) : 0;
	return { rows, selected: safeSelected, moveUp, moveDown, toggle, activate, select };
}
