import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { basename, isAbsolute, join, relative, resolve, sep } from 'node:path';
import type { TreeNode } from '../types.js';

/**
 * A filesystem workspace scoped to a single root directory.
 *
 * Every read and write is constrained to `root`; any path that resolves
 * outside it throws. This makes the editor safe to point at a bundled
 * sample folder — it can never read or overwrite files elsewhere on disk.
 */
export class Workspace {
	readonly root: string;

	constructor(root: string) {
		this.root = resolve(root);
	}

	/** Resolves `target` under the root, throwing if it escapes the root. */
	private guard(target: string): string {
		const abs = resolve(this.root, target);
		const rel = relative(this.root, abs);
		if (rel === '..' || rel.startsWith('..' + sep) || isAbsolute(rel)) {
			throw new Error(`Path escapes workspace root: ${target}`);
		}
		return abs;
	}

	/** Path of `abs` relative to the root, for breadcrumbs. */
	relativePath(abs: string): string {
		return relative(this.root, this.guard(abs)) || basename(this.root);
	}

	/** Reads a file's UTF-8 contents. */
	read(path: string): string {
		return readFileSync(this.guard(path), 'utf8');
	}

	/** Writes UTF-8 contents to a file (backs the editor's save). */
	write(path: string, content: string): void {
		writeFileSync(this.guard(path), content, 'utf8');
	}

	/**
	 * Builds the whole tree under the root eagerly, directories before files
	 * and each group sorted alphabetically. Dotfiles are skipped.
	 *
	 * ponytail: eager full read — fine for a small bundled workspace.
	 * Add lazy per-directory loading only if pointed at a large tree.
	 */
	readTree(): TreeNode {
		return this.readDir(this.root);
	}

	private readDir(dir: string): TreeNode {
		const abs = this.guard(dir);
		const entries = readdirSync(abs, { withFileTypes: true })
			.filter((e) => !e.name.startsWith('.'))
			.sort((a, b) => {
				const dirFirst = Number(b.isDirectory()) - Number(a.isDirectory());
				return dirFirst !== 0 ? dirFirst : a.name.localeCompare(b.name);
			});

		const children: TreeNode[] = entries.map((e) => {
			const childPath = join(abs, e.name);
			return e.isDirectory()
				? this.readDir(childPath)
				: { name: e.name, path: childPath, isDir: false };
		});

		return { name: basename(abs), path: abs, isDir: true, children };
	}
}
