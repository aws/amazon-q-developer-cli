import { execFile, execSync } from 'child_process';
import { promisify } from 'util';

const execFileAsync = promisify(execFile);

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
 *
 * Synchronous variant — blocks the caller for up to 1s. Used at component
 * mount where a sync result avoids a "flash of no branch" on first paint.
 * Anywhere we re-check during a running session (e.g. lite layout's
 * turn-boundary refresh in `LiteLayout.tsx`), use {@link getGitBranchAsync}
 * instead so a slow `git rev-parse` (NFS home, large repo, cold fs cache)
 * can't stall a React render for up to a second.
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

/**
 * Async variant of {@link getGitBranch}. Spawns `git rev-parse` off the
 * render path so a slow filesystem can't pin the UI thread. Same return
 * contract: branch name string, or null on error / not-a-git-repo.
 */
export async function getGitBranchAsync(): Promise<string | null> {
  try {
    const { stdout } = await execFileAsync(
      'git',
      ['rev-parse', '--abbrev-ref', 'HEAD'],
      { encoding: 'utf8', timeout: 1000 }
    );
    const branch = stdout.trim();
    return branch || null;
  } catch {
    return null;
  }
}
