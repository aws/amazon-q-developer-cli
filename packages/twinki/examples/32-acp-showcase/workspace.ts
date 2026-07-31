import { readFileSync, readdirSync, statSync } from "node:fs";
import { basename, extname, relative, resolve, sep } from "node:path";
import { sanitizeTerminalText } from "twinki";
import type { FileEntry, OpenFile } from "./types.js";

const MAX_FILES = 300;
const MAX_DEPTH = 7;
const MAX_BYTES = 400_000;
const IGNORED = new Set([".git", ".idea", ".kiro-studio", "coverage", "dist", "node_modules", "target"]);

const LANGUAGES: Record<string, string> = {
  ".c": "c",
  ".cpp": "cpp",
  ".css": "css",
  ".go": "go",
  ".html": "html",
  ".java": "java",
  ".js": "javascript",
  ".json": "json",
  ".jsx": "jsx",
  ".kt": "kotlin",
  ".md": "markdown",
  ".py": "python",
  ".rs": "rust",
  ".sh": "shellscript",
  ".sql": "sql",
  ".toml": "toml",
  ".ts": "typescript",
  ".tsx": "tsx",
  ".yaml": "yaml",
  ".yml": "yaml",
};

export function scanWorkspace(root: string): FileEntry[] {
  const workspaceRoot = resolve(root);
  const files: FileEntry[] = [];

  const walk = (directory: string, depth: number): void => {
    if (depth > MAX_DEPTH || files.length >= MAX_FILES) return;
    let entries;
    try {
      entries = readdirSync(directory, { withFileTypes: true });
    } catch {
      return;
    }
    entries.sort(
      (left, right) => Number(right.isDirectory()) - Number(left.isDirectory()) || left.name.localeCompare(right.name),
    );
    for (const entry of entries) {
      if (files.length >= MAX_FILES) break;
      if (entry.name.startsWith(".") || IGNORED.has(entry.name)) continue;
      const path = resolve(directory, entry.name);
      if (entry.isSymbolicLink()) continue;
      if (entry.isDirectory()) {
        walk(path, depth + 1);
      } else if (entry.isFile()) {
        const relativePath = relative(workspaceRoot, path);
        files.push({
          path,
          relativePath,
          name: entry.name,
          language: LANGUAGES[extname(entry.name).toLowerCase()],
        });
      }
    }
  };

  walk(workspaceRoot, 0);
  return files.sort((left, right) => left.relativePath.localeCompare(right.relativePath));
}

export function openWorkspaceFile(root: string, entry: FileEntry): OpenFile {
  const workspaceRoot = resolve(root);
  const path = resolve(entry.path);
  const rel = relative(workspaceRoot, path);
  if (rel === ".." || rel.startsWith(`..${sep}`)) {
    return { ...entry, content: "", error: "Path is outside the workspace" };
  }
  try {
    const size = statSync(path).size;
    if (size > MAX_BYTES) {
      return {
        ...entry,
        content: "",
        error: `File exceeds ${Math.round(MAX_BYTES / 1000)} KB`,
      };
    }
    const value = readFileSync(path);
    if (value.subarray(0, 8000).includes(0)) {
      return {
        ...entry,
        content: "",
        error: "Binary file preview is unavailable",
      };
    }
    return {
      ...entry,
      content: sanitizeTerminalText(value.toString("utf8")),
    };
  } catch (error) {
    return {
      ...entry,
      content: "",
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

export function workspaceLabel(root: string): string {
  return basename(resolve(root)) || resolve(root);
}
