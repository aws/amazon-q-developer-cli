/** Shared types for the file-editor example. */

/** A node in the file tree. Directories may carry children. */
export interface TreeNode {
	/** Base name, e.g. "app.tsx". */
	name: string;
	/** Absolute path on disk. */
	path: string;
	/** True for directories. */
	isDir: boolean;
	/** Child nodes (directories only). */
	children?: TreeNode[];
}

/** A single visible row in the flattened, expandable tree. */
export interface VisibleRow {
	/** The node this row renders. */
	node: TreeNode;
	/** Indentation depth (0 = top level). */
	depth: number;
	/** Whether this directory row is currently expanded. */
	expanded: boolean;
}
