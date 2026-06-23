import { execFile as realExecFile } from 'child_process';
import { promisify } from 'util';
import { resolveGitPath, type ResolveGitPathDeps } from './git.js';

const execFileAsync = promisify(realExecFile);
const GIT_TIMEOUT_MS = 1000;

/**
 * Async variant of getGitBranch. Spawns `git rev-parse` off the render path so
 * a slow filesystem cannot pin the UI thread. Same return contract: branch
 * name string, or null on error / not-a-git-repo.
 */
export async function getGitBranchAsync(
  deps: ResolveGitPathDeps = {}
): Promise<string | null> {
  try {
    const gitPath = resolveGitPath(deps);
    if (gitPath === null) return null;
    const { stdout } = await execFileAsync(
      gitPath,
      ['rev-parse', '--abbrev-ref', 'HEAD'],
      {
        shell: false,
        encoding: 'utf8',
        timeout: GIT_TIMEOUT_MS,
      }
    );
    const branch = String(stdout).trim();
    return branch || null;
  } catch {
    return null;
  }
}
