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

/**
 * Recursively search for files matching query
 */
function searchFilesRecursive(
  dir: string,
  query: string,
  depth: number,
  maxDepth: number,
  limit: number,
  results: string[],
  basePath: string,
  ig: ReturnType<typeof ignore>
): void {
  if (depth > maxDepth || results.length >= limit) return;
  
  try {
    const entries = readdirSync(dir, { withFileTypes: true });
    
    for (const entry of entries) {
      if (results.length >= limit) break;
      
      const fullPath = join(dir, entry.name);
      const relativePath = relative(basePath, fullPath);
      
      // Check if ignored by .gitignore rules
      if (ig.ignores(relativePath)) continue;
      
      if (entry.isDirectory()) {
        searchFilesRecursive(fullPath, query, depth + 1, maxDepth, limit, results, basePath, ig);
      } else if (entry.isFile()) {
        // Case-insensitive match
        if (entry.name.toLowerCase().includes(query.toLowerCase())) {
          results.push(relativePath);
        }
      }
    }
  } catch {
    // Skip directories we can't read
  }
}

/**
 * Search for files matching a query (case insensitive).
 * Returns up to `limit` file paths relative to cwd.
 * Respects .gitignore patterns.
 */
export function searchFiles(query: string, limit = 20): string[] {
  if (!query) return [];
  
  const cwd = process.cwd();
  const ig = loadGitignore(cwd);
  const results: string[] = [];
  
  searchFilesRecursive(cwd, query, 0, MAX_SEARCH_DEPTH, limit, results, cwd, ig);
  
  return results;
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
