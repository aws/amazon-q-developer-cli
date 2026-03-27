/**
 * File search utilities.
 *
 * - searchFilesAsync: auto-cancelling async search for the UI (CommandMenu)
 * - searchFilesAbortable: lower-level async search with explicit AbortSignal
 * - readFileContent / expandFileReferences: file content helpers for message submission
 */

import { readFileSync, existsSync } from 'fs';
import { opendir } from 'fs/promises';
import { join, relative } from 'path';
import ignore from 'ignore';

const MAX_SEARCH_DEPTH = 5;
const MAX_COLLECT = 200;

function loadGitignore(cwd: string) {
  const ig = ignore();

  ig.add([
    // Build outputs
    'build',
    'dist',
    'out',
    'target',
    // Dependencies
    'node_modules',
    'vendor',
    '.venv',
    'venv',
    '__pycache__',
    // IDE/Tools
    '.idea',
    '.vscode',
    '.git',
    // Package caches
    '.cache',
    '.gradle',
    '.npm',
    '.cargo',
  ]);

  const gitignorePath = join(cwd, '.gitignore');
  if (existsSync(gitignorePath)) {
    try {
      ig.add(readFileSync(gitignorePath, 'utf-8'));
    } catch {
      // ignore
    }
  }

  return ig;
}

function rankMatch(filePath: string, query: string): number {
  const lowerQuery = query.toLowerCase();
  const fileName = filePath.split('/').pop()?.toLowerCase() ?? '';
  const depth = filePath.split('/').length;

  if (fileName.startsWith(lowerQuery)) return depth;
  if (fileName.includes(lowerQuery)) return 100 + depth;
  return 200 + depth;
}

// ---------------------------------------------------------------------------
// Async cancellable file search
// ---------------------------------------------------------------------------

async function collectMatchingFiles(
  dir: string,
  query: string,
  depth: number,
  maxDepth: number,
  maxCollect: number,
  results: string[],
  basePath: string,
  ig: ReturnType<typeof ignore>,
  signal: AbortSignal
): Promise<void> {
  if (depth > maxDepth || results.length >= maxCollect || signal.aborted)
    return;

  let dirHandle;
  try {
    dirHandle = await opendir(dir);
  } catch {
    return;
  }

  try {
    for await (const entry of dirHandle) {
      if (signal.aborted || results.length >= maxCollect) break;

      const fullPath = join(dir, entry.name);
      const relativePath = relative(basePath, fullPath);

      if (ig.ignores(relativePath)) continue;

      if (entry.isDirectory()) {
        await collectMatchingFiles(
          fullPath,
          query,
          depth + 1,
          maxDepth,
          maxCollect,
          results,
          basePath,
          ig,
          signal
        );
      } else if (entry.isFile()) {
        const lowerName = entry.name.toLowerCase();
        const lowerPath = relativePath.toLowerCase();
        const lowerQuery = query.toLowerCase();
        if (lowerName.includes(lowerQuery) || lowerPath.includes(lowerQuery)) {
          results.push(relativePath);
        }
      }
    }
  } catch {
    // dir deleted mid-walk, etc.
  }
}

/**
 * Async cancellable file search with explicit AbortSignal.
 * Uses fs.promises.opendir — yields to the event loop between entries.
 */
export async function searchFilesAbortable(
  query: string,
  signal: AbortSignal,
  limit = 20
): Promise<string[]> {
  if (!query || signal.aborted) return [];

  const cwd = process.cwd();
  const ig = loadGitignore(cwd);
  const results: string[] = [];

  await collectMatchingFiles(
    cwd,
    query,
    0,
    MAX_SEARCH_DEPTH,
    MAX_COLLECT,
    results,
    cwd,
    ig,
    signal
  );

  if (signal.aborted) return [];

  results.sort((a, b) => rankMatch(a, query) - rankMatch(b, query));
  return results.slice(0, limit);
}

// ---------------------------------------------------------------------------
// File content helpers (used at message submission time)
// ---------------------------------------------------------------------------

/** Read file content safely. Returns null on error. */
export function readFileContent(filePath: string): string | null {
  try {
    if (!existsSync(filePath)) return null;
    return readFileSync(filePath, 'utf-8');
  } catch {
    return null;
  }
}

/** Expand @file:path references in content to include file contents. */
export function expandFileReferences(content: string): string {
  const fileRefPattern = /@file:(\S+)/g;

  return content.replace(fileRefPattern, (match, filePath) => {
    if (!existsSync(filePath)) return match;

    try {
      const fileContent = readFileSync(filePath, 'utf-8');
      return `<attached_file path="${filePath}">\n${fileContent}\n</attached_file>`;
    } catch {
      return match;
    }
  });
}
