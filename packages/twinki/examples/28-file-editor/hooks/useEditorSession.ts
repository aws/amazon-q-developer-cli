import { useCallback, useState } from 'react';
import { languageForFile } from '../lib/language.js';
import type { Workspace } from '../lib/workspace.js';
import type { TreeNode } from '../types.js';

export interface EditorSession {
	/** Absolute path of the open file, or null when none is open. */
	path: string | null;
	/** Base name of the open file. */
	name: string | null;
	/** Path relative to the workspace root (for the breadcrumb). */
	relPath: string | null;
	/** shiki language id, or undefined for plain text. */
	language: string | undefined;
	/** Current (possibly unsaved) buffer contents. */
	value: string;
	/** True when the buffer differs from what's on disk. */
	dirty: boolean;
	/** Load a file into the buffer (ignores directories). */
	open: (node: TreeNode) => void;
	/** Update the buffer as the user types. */
	change: (value: string) => void;
	/** Persist the buffer to disk. */
	save: () => void;
}

/**
 * Editor buffer state for a single open file: loading, tracking unsaved edits
 * (dirty), and persisting via the Workspace. One responsibility — the open
 * "document" — kept separate from tree navigation and theming.
 */
export function useEditorSession(workspace: Workspace): EditorSession {
	const [path, setPath] = useState<string | null>(null);
	const [name, setName] = useState<string | null>(null);
	const [relPath, setRelPath] = useState<string | null>(null);
	const [language, setLanguage] = useState<string | undefined>(undefined);
	const [value, setValue] = useState('');
	const [saved, setSaved] = useState('');

	const open = useCallback(
		(node: TreeNode) => {
			if (node.isDir) return;
			const text = workspace.read(node.path);
			setPath(node.path);
			setName(node.name);
			setRelPath(workspace.relativePath(node.path));
			setLanguage(languageForFile(node.name));
			setValue(text);
			setSaved(text);
		},
		[workspace],
	);

	const change = useCallback((next: string) => setValue(next), []);

	const save = useCallback(() => {
		if (path === null) return;
		workspace.write(path, value);
		setSaved(value);
	}, [workspace, path, value]);

	return {
		path,
		name,
		relPath,
		language,
		value,
		dirty: path !== null && value !== saved,
		open,
		change,
		save,
	};
}
