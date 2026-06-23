import { describe, test, expect, mock } from 'bun:test';
import { win32 } from 'path';
import {
  resolveGitPath,
  getGitBranch,
  type ResolveGitPathDeps,
  type GetGitBranchDeps,
} from './git.js';

/**
 * Tests for the Windows untrusted-search-path hardening (CWE-426) in
 * `resolveGitPath`/`getGitBranch`.
 *
 * Rather than mocking `node:fs`/`node:child_process`/`node:path` (bun's
 * `mock.module` is process-global and leaks across test files), the Windows
 * code path is exercised deterministically on a POSIX host via DEPENDENCY
 * INJECTION: each test passes a `deps` object with fake `existsSync`,
 * `realpathSyncNative`, `platform`, `env`, `cwd`, and (for `getGitBranch`) an
 * `execFileSync` spy. The source itself uses `path.win32` explicitly on the
 * Windows branch, so no `path` mock is required. `win32` is imported here only
 * to build expected Windows paths in assertions.
 *
 * Note the source takes NO `execSync`/`spawn` dependency: a shell-based (and
 * thus hijackable) git resolution is structurally impossible, so its absence
 * is itself the security guarantee.
 */

/**
 * Builds a fake fs/env/platform deps object for `resolveGitPath`/`getGitBranch`
 * without any module mocking, so nothing leaks into other test files.
 */
function fsDeps(opts: {
  platform: NodeJS.Platform;
  cwd?: string;
  env?: NodeJS.ProcessEnv;
  existing?: Iterable<string>;
  realpathMap?: Map<string, string>;
}): ResolveGitPathDeps {
  const existing = new Set(opts.existing ?? []);
  // Maps an input path to the canonical path `realpathSync.native` should
  // return; defaults to identity so the realpath-based CWD exclusion can be
  // exercised without a real filesystem. An entry models an 8.3 short-name /
  // NTFS junction that aliases another directory.
  const realpathMap = opts.realpathMap ?? new Map<string, string>();
  return {
    platform: opts.platform,
    env: opts.env ?? {},
    cwd: () => opts.cwd ?? '/',
    existsSync: (p: string) => existing.has(p),
    realpathSyncNative: (p: string) => realpathMap.get(p) ?? p,
  };
}

/** Builds an execFileSync spy returning `result` (or throwing). */
function execFileSpy(result: string, throws = false) {
  return mock(
    (_file: string, _args?: readonly string[], _opts?: unknown): string => {
      if (throws) throw new Error('git failed');
      return result;
    }
  );
}

/**
 * Asserts the resolution never invoked a shell or `where.exe`. The source only
 * ever runs the resolved binary via the injected `execFileSync`; assert each
 * such call targeted neither `where` nor a shell.
 */
function assertNoShellOrWhere(spy?: ReturnType<typeof execFileSpy>): void {
  if (!spy) return;
  for (const call of spy.mock.calls) {
    const file = String(call[0]).toLowerCase();
    expect(file).not.toContain('where');
    expect(file).not.toContain('cmd.exe');
  }
}

describe('resolveGitPath (Windows)', () => {
  const cwd = 'C:\\projects\\evil';

  test('prefers a known ProgramFiles install path when it exists', () => {
    const expected = win32.join('C:\\Program Files', 'Git', 'cmd', 'git.exe');
    const deps = fsDeps({
      platform: 'win32',
      cwd,
      env: { ProgramFiles: 'C:\\Program Files' },
      existing: [expected],
    });

    expect(resolveGitPath(deps)).toBe(expected);
  });

  test('falls back to the per-user LOCALAPPDATA install path', () => {
    const expected = win32.join(
      'C:\\Users\\me\\AppData\\Local',
      'Programs',
      'Git',
      'cmd',
      'git.exe'
    );
    const deps = fsDeps({
      platform: 'win32',
      cwd,
      env: {
        ProgramFiles: 'C:\\Program Files',
        LOCALAPPDATA: 'C:\\Users\\me\\AppData\\Local',
      },
      // Only the LOCALAPPDATA path exists, so the known ProgramFiles paths miss.
      existing: [expected],
    });

    expect(resolveGitPath(deps)).toBe(expected);
  });

  test('PATH scan selects first absolute dir with git.exe and NEVER the CWD or relative entries', () => {
    // No known install path exists, forcing the manual PATH scan.
    const legitDir = 'C:\\tools\\git\\bin';
    const cwdGit = win32.join(cwd, 'git.exe');
    const dotGit = win32.join('.', 'git.exe');
    const relGit = win32.join('relative\\sub', 'git.exe');
    const legitGit = win32.join(legitDir, 'git.exe');
    const deps = fsDeps({
      platform: 'win32',
      cwd,
      env: {
        ProgramFiles: 'C:\\Program Files',
        PATH: [cwd, '.', '', '   ', 'relative\\sub', legitDir].join(';'),
      },
      // Plant a malicious git.exe in the CWD and relative/`.` entries, plus a
      // legitimate one in an absolute PATH dir.
      existing: [cwdGit, dotGit, relGit, legitGit],
    });

    const resolved = resolveGitPath(deps);

    // Core security assertion: the CWD-planted binary is never selected.
    expect(resolved).toBe(legitGit);
    expect(resolved).not.toBe(cwdGit);
    expect(resolved).not.toBe(dotGit);
    expect(resolved).not.toBe(relGit);
  });

  test('returns null when no trusted git binary is found (fail closed)', () => {
    const deps = fsDeps({
      platform: 'win32',
      cwd,
      env: { PATH: [cwd, '.', 'relative\\dir'].join(';') },
      // Even a git.exe in the CWD must not produce a result.
      existing: [win32.join(cwd, 'git.exe')],
    });

    expect(resolveGitPath(deps)).toBeNull();
  });

  test('S2: selects a QUOTED absolute PATH dir containing git.exe', () => {
    // Windows PATH entries are commonly wrapped in double quotes. The
    // surrounding pair must be stripped, otherwise isAbsolute() rejects the
    // entry and a legitimate git install is hidden.
    const quotedDir = 'C:\\Program Files\\Git\\cmd';
    const legitGit = win32.join(quotedDir, 'git.exe');
    const deps = fsDeps({
      platform: 'win32',
      cwd,
      env: { ProgramFiles: 'C:\\Program Files', PATH: `"${quotedDir}"` },
      existing: [legitGit],
    });

    expect(resolveGitPath(deps)).toBe(legitGit);
  });

  test('S3: skips PATH entries that alias the CWD (trailing slash, case, junction/short-name)', () => {
    const legitDir = 'C:\\tools\\git\\bin';
    // Variants that all resolve to the CWD:
    //  - trailing slash + different case are caught by the cheap lexical check;
    //  - the short-name/junction alias is caught only by the realpath compare,
    //    modelled here by mapping the alias dir to the real CWD.
    const trailingSlash = 'C:\\projects\\evil\\';
    const caseVariant = 'c:\\PROJECTS\\EVIL';
    const junctionAlias = 'C:\\PROGRA~1\\evil-junction';
    const legitGit = win32.join(legitDir, 'git.exe');
    const deps = fsDeps({
      platform: 'win32',
      cwd,
      env: {
        ProgramFiles: 'C:\\Program Files',
        PATH: [trailingSlash, caseVariant, junctionAlias, legitDir].join(';'),
      },
      // Plant a malicious git.exe in every CWD-alias dir plus a legit one later.
      existing: [
        win32.join(cwd, 'git.exe'),
        win32.join(junctionAlias, 'git.exe'),
        legitGit,
      ],
      realpathMap: new Map([[junctionAlias, cwd]]),
    });

    // Only the genuinely distinct legit dir is selected; all CWD aliases skip.
    expect(resolveGitPath(deps)).toBe(legitGit);
  });

  test('S3: a junction/short-name aliasing the CWD fails closed when it is the only git', () => {
    const junctionAlias = 'C:\\PROGRA~1\\evil-junction';
    const deps = fsDeps({
      platform: 'win32',
      cwd,
      env: { ProgramFiles: 'C:\\Program Files', PATH: junctionAlias },
      // git.exe exists only inside the CWD-aliasing dir → must not be selected.
      existing: [win32.join(junctionAlias, 'git.exe')],
      realpathMap: new Map([[junctionAlias, cwd]]),
    });

    expect(resolveGitPath(deps)).toBeNull();
  });
});

describe('resolveGitPath (Unix)', () => {
  test('returns bare "git"', () => {
    expect(resolveGitPath({ platform: 'linux' })).toBe('git');
  });
});

describe('getGitBranch', () => {
  test('Windows: invokes resolved absolute git via execFileSync (args array, shell:false)', () => {
    const gitPath = win32.join('C:\\Program Files', 'Git', 'cmd', 'git.exe');
    const execFileSync = execFileSpy('feature/x\n');
    const deps: GetGitBranchDeps = {
      ...fsDeps({
        platform: 'win32',
        cwd: 'C:\\projects\\evil',
        env: { ProgramFiles: 'C:\\Program Files' },
        existing: [gitPath],
      }),
      execFileSync: execFileSync as unknown as GetGitBranchDeps['execFileSync'],
    };

    expect(getGitBranch(deps)).toBe('feature/x');
    expect(execFileSync).toHaveBeenCalledTimes(1);
    const [file, args, opts] = execFileSync.mock.calls[0] as [
      string,
      string[],
      { shell?: boolean },
    ];
    expect(file).toBe(gitPath);
    expect(args).toEqual(['rev-parse', '--abbrev-ref', 'HEAD']);
    expect(opts.shell).toBe(false);
    assertNoShellOrWhere(execFileSync);
  });

  test('Unix: invokes "git" via execFileSync with args array and no shell', () => {
    const execFileSync = execFileSpy('main\n');
    const deps: GetGitBranchDeps = {
      platform: 'linux',
      execFileSync: execFileSync as unknown as GetGitBranchDeps['execFileSync'],
    };

    expect(getGitBranch(deps)).toBe('main');
    expect(execFileSync).toHaveBeenCalledTimes(1);
    const [file, args, opts] = execFileSync.mock.calls[0] as [
      string,
      string[],
      { shell?: boolean },
    ];
    expect(file).toBe('git');
    expect(args).toEqual(['rev-parse', '--abbrev-ref', 'HEAD']);
    expect(opts.shell).toBe(false);
    assertNoShellOrWhere(execFileSync);
  });

  test('Windows: returns null without exec when git cannot be resolved', () => {
    const execFileSync = execFileSpy('');
    const deps: GetGitBranchDeps = {
      ...fsDeps({
        platform: 'win32',
        cwd: 'C:\\projects\\evil',
        env: { PATH: 'C:\\projects\\evil' },
        // Nothing exists, so resolveGitPath returns null.
      }),
      execFileSync: execFileSync as unknown as GetGitBranchDeps['execFileSync'],
    };

    expect(getGitBranch(deps)).toBeNull();
    expect(execFileSync).not.toHaveBeenCalled();
  });

  test('returns null when git invocation throws', () => {
    const execFileSync = execFileSpy('', true);
    const deps: GetGitBranchDeps = {
      platform: 'linux',
      execFileSync: execFileSync as unknown as GetGitBranchDeps['execFileSync'],
    };

    expect(getGitBranch(deps)).toBeNull();
    assertNoShellOrWhere(execFileSync);
  });

  test('returns null when git outputs an empty branch', () => {
    const execFileSync = execFileSpy('   \n');
    const deps: GetGitBranchDeps = {
      platform: 'linux',
      execFileSync: execFileSync as unknown as GetGitBranchDeps['execFileSync'],
    };

    expect(getGitBranch(deps)).toBeNull();
  });
});
