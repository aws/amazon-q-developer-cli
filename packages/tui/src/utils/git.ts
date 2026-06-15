import { execSync } from 'child_process';

/**
 * Resolves the full path to the git binary.
 * On Windows, bare command names resolve CWD first (CWE-426), so we use
 * where.exe to find the real git binary from PATH only.
 * On Unix, CWD is not searched by default, so bare 'git' is safe.
 */
function resolveGitPath(): string {
  if (process.platform === 'win32') {
    const result = execSync('where.exe git', {
      encoding: 'utf8',
      stdio: ['pipe', 'pipe', 'ignore'],
      timeout: 1000,
    }).trim();
    // where.exe may return multiple lines; take the first match
    const firstLine = result.split(/\r?\n/)[0];
    if (!firstLine) throw new Error('where.exe returned empty result');
    return firstLine;
  }
  return 'git';
}

/**
 * Gets the current git branch name.
 * Returns null if not in a git repository or if git is not available.
 */
export function getGitBranch(): string | null {
  try {
    const gitPath = resolveGitPath();
    const branch = execSync(`"${gitPath}" rev-parse --abbrev-ref HEAD`, {
      encoding: 'utf8',
      stdio: ['pipe', 'pipe', 'ignore'],
      timeout: 1000,
    }).trim();
    return branch || null;
  } catch {
    return null;
  }
}
