import { execSync } from 'child_process';

/**
 * Gets the current git branch name.
 * Returns null if not in a git repository or if git is not available.
 */
export function getGitBranch(): string | null {
  try {
    const branch = execSync('git rev-parse --abbrev-ref HEAD', {
      encoding: 'utf8',
      stdio: ['pipe', 'pipe', 'ignore'],
      timeout: 1000,
    }).trim();
    return branch || null;
  } catch {
    return null;
  }
}
