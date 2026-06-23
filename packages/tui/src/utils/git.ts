import { execFile, execFileSync as realExecFileSync } from 'child_process';
import { existsSync as realExistsSync, realpathSync } from 'fs';
import { win32 } from 'path';
import { promisify } from 'util';

const execFileAsync = promisify(execFile);

/** Timeout for the branch lookup. Kept short: branch display is cosmetic. */
const GIT_TIMEOUT_MS = 1000;

/**
 * Injectable dependencies for {@link resolveGitPath}. Every field is optional
 * and defaults to the real runtime implementation, so production callers pass
 * nothing while tests can drive the Windows code path deterministically on a
 * POSIX host without process-global module mocking.
 */
export interface ResolveGitPathDeps {
  platform?: NodeJS.Platform;
  env?: NodeJS.ProcessEnv;
  cwd?: () => string;
  existsSync?: (p: string) => boolean;
  realpathSyncNative?: (p: string) => string;
}

/**
 * Injectable dependencies for {@link getGitBranch}: the {@link ResolveGitPathDeps}
 * plus the `execFileSync` used to run the resolved binary. Note there is no
 * `execSync`/`spawn` dependency by design — git resolution must never use a
 * shell, and the absence of any such dependency makes a shell-based (and thus
 * hijackable) resolution structurally impossible.
 */
export interface GetGitBranchDeps extends ResolveGitPathDeps {
  execFileSync?: typeof realExecFileSync;
}

/**
 * Resolves the full path to the git binary without trusting the current
 * working directory or relative PATH entries (CWE-426 untrusted search path
 * → RCE). This runs at TUI startup before the opened project is trusted, so a
 * malicious `.\git.exe` (or `.\where.exe`) planted in the project must never be
 * selected.
 *
 * Windows: prefer known absolute install locations, then fall back to a manual
 * PATH scan that we control — skipping the CWD and any relative/empty entries.
 * `where.exe` is deliberately NOT used: it searches the CWD before PATH and,
 * when invoked unqualified through a shell, is itself hijackable. Returns null
 * if no trusted git binary is found, so callers can fail closed rather than
 * execute an untrusted binary.
 *
 * The Windows branch uses `path.win32` explicitly rather than the ambient
 * `path` module. On a real Windows host `path === path.win32`, so behavior is
 * identical; on POSIX (CI) it keeps the Windows semantics correct and testable
 * without mocking `node:path`.
 *
 * Unix: the default PATH does not include the CWD, so a bare 'git' is safe.
 */
export function resolveGitPath(deps: ResolveGitPathDeps = {}): string | null {
  const platform = deps.platform ?? process.platform;
  const env = deps.env ?? process.env;
  const cwd = deps.cwd ?? (() => process.cwd());
  const existsSync = deps.existsSync ?? realExistsSync;
  const realpathSyncNative =
    deps.realpathSyncNative ?? ((p: string) => realpathSync.native(p));

  if (platform !== 'win32') {
    return 'git';
  }

  const programFiles = env.ProgramFiles ?? 'C:\\Program Files';
  const programFilesX86 = env['ProgramFiles(x86)'] ?? 'C:\\Program Files (x86)';
  const localAppData = env.LOCALAPPDATA;

  // 1. Known absolute install paths take precedence.
  const knownPaths = [
    win32.join(programFiles, 'Git', 'cmd', 'git.exe'),
    win32.join(programFiles, 'Git', 'bin', 'git.exe'),
    win32.join(programFilesX86, 'Git', 'cmd', 'git.exe'),
    ...(localAppData
      ? [win32.join(localAppData, 'Programs', 'Git', 'cmd', 'git.exe')]
      : []),
  ];
  for (const candidate of knownPaths) {
    if (existsSync(candidate)) return candidate;
  }

  // 2. Fallback: scan PATH ourselves so we control which directories are
  // trusted. Only non-empty, absolute directories that are not the CWD and not
  // relative references (`.`/`..`) are searched — the project directory and any
  // relative PATH entry can therefore never be selected.
  const cwdLower = win32.resolve(cwd()).toLowerCase();
  // Canonical (symlink/junction/8.3-short-name resolved) form of the CWD. On
  // Windows the project directory can be reached through an NTFS junction or a
  // short-name like `PROGRA~1`, which the cheap lexical compare below would
  // miss; comparing real paths ensures a PATH entry that merely aliases the
  // project directory is still excluded. Fall back to the lexical form if
  // realpath is unavailable.
  let realCwd: string;
  try {
    realCwd = realpathSyncNative(cwd()).toLowerCase();
  } catch {
    realCwd = cwdLower;
  }

  const entries = (env.PATH ?? '').split(';');
  for (const entry of entries) {
    // PATH directories are frequently wrapped in double quotes (e.g.
    // "C:\Program Files\Git\cmd"). Strip a single surrounding pair before the
    // checks below so a legitimately quoted directory is not rejected by the
    // isAbsolute() guard and a real git install is not hidden.
    let dir = entry.trim();
    if (dir.length >= 2 && dir.startsWith('"') && dir.endsWith('"')) {
      dir = dir.slice(1, -1);
    }
    if (dir === '' || dir === '.' || dir === '..') continue;
    if (!win32.isAbsolute(dir)) continue;
    // Cheap lexical CWD check first.
    if (win32.resolve(dir).toLowerCase() === cwdLower) continue;
    // Then a canonical real-path check to catch short-names/junctions that
    // alias the project directory. If the directory cannot be resolved (e.g. it
    // does not exist) skip it — there is no git binary to find there anyway.
    let realDir: string;
    try {
      realDir = realpathSyncNative(dir).toLowerCase();
    } catch {
      continue;
    }
    if (realDir === realCwd) continue;
    const candidate = win32.join(dir, 'git.exe');
    if (existsSync(candidate)) return candidate;
  }

  // 3. Nothing trusted found: fail closed (never fall back to a bare 'git').
  return null;
}

/**
 * Gets the current git branch name.
 * Returns null if not in a git repository, if git cannot be safely resolved,
 * or if git is not available.
 *
 * Synchronous variant — blocks the caller for up to 1s. Used at component
 * mount where a sync result avoids a "flash of no branch" on first paint.
 * Anywhere we re-check during a running session (e.g. lite layout's
 * turn-boundary refresh in `LiteLayout.tsx`), use {@link getGitBranchAsync}
 * instead so a slow `git rev-parse` (NFS home, large repo, cold fs cache)
 * can't stall a React render for up to a second.
 */
export function getGitBranch(deps: GetGitBranchDeps = {}): string | null {
  try {
    const gitPath = resolveGitPath(deps);
    if (gitPath === null) return null;
    const execFileSync = deps.execFileSync ?? realExecFileSync;
    const branch = execFileSync(
      gitPath,
      ['rev-parse', '--abbrev-ref', 'HEAD'],
      {
        shell: false,
        encoding: 'utf8',
        stdio: ['pipe', 'pipe', 'ignore'],
        timeout: GIT_TIMEOUT_MS,
      }
    ).trim();
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
    const gitPath = resolveGitPath();
    if (gitPath === null) return null;
    const { stdout } = await execFileAsync(
      gitPath,
      ['rev-parse', '--abbrev-ref', 'HEAD'],
      { shell: false, encoding: 'utf8', timeout: GIT_TIMEOUT_MS }
    );
    const branch = stdout.trim();
    return branch || null;
  } catch {
    return null;
  }
}
