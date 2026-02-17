/**
 * Pure helpers for parsing `ls -la` style output from the Rust LS tool.
 *
 * Line format produced by `Entry::to_long_format`:
 *   "{ftype}{mode} {nlink} {uid} {gid} {size} {Mon} {DD} {HH:MM} {path}"
 *    0              1       2     3     4      5     6    7        8...
 *
 * The date occupies 3 whitespace-separated tokens (month, day, time).
 */

const PATH_TOKEN_INDEX = 8;

/**
 * Filter raw LS output text into actual entry lines, stripping prefix
 * metadata lines (User id, truncation notices).
 */
export function parseLsEntries(text: string): string[] {
  return text
    .split('\n')
    .filter(
      (line) =>
        line.length > 0 &&
        !line.startsWith('User id:') &&
        !line.startsWith('Directory at ')
    );
}

/**
 * Extract the basename (filename) from an ls long-format line.
 */
export function getEntryName(line: string): string {
  const parts = line.trimEnd().split(/\s+/);
  if (parts.length >= PATH_TOKEN_INDEX + 1) {
    const fullPath = parts.slice(PATH_TOKEN_INDEX).join(' ');
    const lastSlash = fullPath.lastIndexOf('/');
    return lastSlash >= 0 ? fullPath.substring(lastSlash + 1) : fullPath;
  }
  return parts[parts.length - 1] || line;
}

/**
 * Resolve the display path for the LS tool. When the raw arg is relative
 * (e.g. "."), derive the actual directory from the first entry's absolute path.
 */
export function resolveLsDisplayPath(
  rawDirPath: string | null,
  entries: string[]
): string | null {
  if (rawDirPath && rawDirPath !== '.' && rawDirPath.startsWith('/')) {
    return rawDirPath;
  }
  const firstEntry = entries[0];
  if (firstEntry) {
    const parts = firstEntry.trimEnd().split(/\s+/);
    if (parts.length >= PATH_TOKEN_INDEX + 1) {
      const fullPath = parts.slice(PATH_TOKEN_INDEX).join(' ');
      const lastSlash = fullPath.lastIndexOf('/');
      if (lastSlash > 0) {
        return fullPath.substring(0, lastSlash);
      }
    }
  }
  return rawDirPath;
}
