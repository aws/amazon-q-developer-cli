/** Maps file extensions to shiki language ids for syntax highlighting. */
const EXT_TO_LANG: Record<string, string> = {
	'.ts': 'typescript',
	'.tsx': 'tsx',
	'.js': 'javascript',
	'.jsx': 'jsx',
	'.mjs': 'javascript',
	'.cjs': 'javascript',
	'.json': 'json',
	'.css': 'css',
	'.scss': 'scss',
	'.html': 'html',
	'.md': 'markdown',
	'.sh': 'bash',
	'.bash': 'bash',
	'.yml': 'yaml',
	'.yaml': 'yaml',
	'.py': 'python',
	'.rs': 'rust',
	'.go': 'go',
};

/**
 * Returns the shiki language id for a file name, or `undefined` when the
 * extension is unknown. `undefined` disables highlighting so the editor
 * falls back to plain text.
 */
export function languageForFile(name: string): string | undefined {
	const dot = name.lastIndexOf('.');
	if (dot < 0) return undefined;
	return EXT_TO_LANG[name.slice(dot).toLowerCase()];
}
