import { execSync } from 'child_process';
import { readFileSync, existsSync } from 'fs';

/**
 * Search for files matching a query using fd or find (case insensitive).
 * Returns up to `limit` file paths relative to cwd.
 */
export function searchFiles(query: string, limit = 20): string[] {
  if (!query) return [];

  try {
    const result = execSync(
      `fd --type f --hidden --exclude .git --ignore-case --max-results ${limit} "${query}" 2>/dev/null || find . -type f -iname "*${query}*" 2>/dev/null | head -${limit}`,
      { encoding: 'utf-8', maxBuffer: 1024 * 1024 }
    );
    return result
      .trim()
      .split('\n')
      .filter(Boolean)
      .map((p) => p.replace(/^\.\//, ''));
  } catch {
    return [];
  }
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
