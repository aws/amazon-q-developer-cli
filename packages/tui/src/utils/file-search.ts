import { readdirSync, readFileSync, existsSync } from 'fs';
import { join, relative } from 'path';
import ignore from 'ignore';

/** Maximum directory depth for file search */
const MAX_SEARCH_DEPTH = 5;

/**
 * Load .gitignore rules
 */
function loadGitignore(cwd: string) {
  const ig = ignore();

  // Always ignore these (matches code-agent-sdk ALWAYS_SKIP_DIRS and fs_read DEFAULT_EXCLUDE_PATTERNS)
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

  // Load .gitignore if it exists
  const gitignorePath = join(cwd, '.gitignore');
  if (existsSync(gitignorePath)) {
    try {
      const content = readFileSync(gitignorePath, 'utf-8');
      ig.add(content);
    } catch {
      // Ignore errors reading .gitignore
    }
  }

  return ig;
}

/** Max results to collect before sorting and truncating */
const MAX_COLLECT = 200;

/**
 * Recursively collect all files matching query (no early limit cutoff)
 */
function collectMatchingFiles(
  dir: string,
  query: string,
  depth: number,
  maxDepth: number,
  maxCollect: number,
  results: string[],
  basePath: string,
  ig: ReturnType<typeof ignore>
): void {
  if (depth > maxDepth || results.length >= maxCollect) return;

  try {
    const entries = readdirSync(dir, { withFileTypes: true });

    for (const entry of entries) {
      if (results.length >= maxCollect) break;

      const fullPath = join(dir, entry.name);
      const relativePath = relative(basePath, fullPath);

      if (ig.ignores(relativePath)) continue;

      if (entry.isDirectory()) {
        collectMatchingFiles(
          fullPath,
          query,
          depth + 1,
          maxDepth,
          maxCollect,
          results,
          basePath,
          ig
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
    // Skip directories we can't read
  }
}

/**
 * Rank a file path by relevance to query.
 * Lower score = better match.
 */
function rankMatch(filePath: string, query: string): number {
  const lowerQuery = query.toLowerCase();
  const fileName = filePath.split('/').pop()?.toLowerCase() ?? '';
  const depth = filePath.split('/').length;

  // Filename starts with query
  if (fileName.startsWith(lowerQuery)) return depth;
  // Filename contains query
  if (fileName.includes(lowerQuery)) return 100 + depth;
  // Path contains query
  return 200 + depth;
}

/**
 * Search for files matching a query (case insensitive).
 * Returns up to `limit` file paths relative to cwd, ranked by relevance.
 * Respects .gitignore patterns.
 */
export function searchFiles(query: string, limit = 20): string[] {
  if (!query) return [];

  const cwd = process.cwd();
  const ig = loadGitignore(cwd);
  const results: string[] = [];

  collectMatchingFiles(
    cwd,
    query,
    0,
    MAX_SEARCH_DEPTH,
    MAX_COLLECT,
    results,
    cwd,
    ig
  );

  results.sort((a, b) => rankMatch(a, query) - rankMatch(b, query));

  return results.slice(0, limit);
}

/**
 * Read file content safely. Returns null on error.
 */
export function readFileContent(filePath: string): string | null {
  try {
    if (!existsSync(filePath)) return null;
    return readFileSync(filePath, 'utf-8');
  } catch {
    return null;
  }
}

/**
 * Expand @file:path references in content to include file contents.
 * Returns the expanded content with file contents wrapped in XML tags.
 */
export function expandFileReferences(content: string): string {
  const fileRefPattern = /@file:(\S+)/g;

  return content.replace(fileRefPattern, (match, filePath) => {
    if (!existsSync(filePath)) {
      return match; // Keep original if file doesn't exist
    }

    try {
      const fileContent = readFileSync(filePath, 'utf-8');
      return `<attached_file path="${filePath}">\n${fileContent}\n</attached_file>`;
    } catch {
      return match; // Keep original on read error
    }
  });
}
