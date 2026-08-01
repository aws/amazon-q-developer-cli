import { execFile } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import { posix, resolve } from 'node:path';
import { sanitizeTerminalText } from 'twinki';

const MAX_GIT_OUTPUT = 2_000_000;

export interface GitFile {
  path: string;
  previousPath?: string;
  code: string;
  staged: boolean;
  unstaged: boolean;
  untracked: boolean;
}

export interface GitSnapshot {
  branch: string;
  files: GitFile[];
  loading: boolean;
  error?: string;
}

function runGit(cwd: string, args: string[]): Promise<string> {
  return new Promise((resolve, reject) => {
    execFile(
      'git',
      ['-c', 'color.ui=false', ...args],
      {
        cwd,
        encoding: 'utf8',
        timeout: 5_000,
        maxBuffer: MAX_GIT_OUTPUT,
      },
      (error, stdout, stderr) => {
        if (error) {
          reject(new Error(stderr.trim() || error.message));
          return;
        }
        resolve(stdout);
      }
    );
  });
}

function parseStatus(value: string, prefix: string): GitFile[] {
  const records = value.split('\0');
  const files: GitFile[] = [];
  const relativeToWorkspace = (path: string): string => posix.relative(prefix || '.', path);
  for (let index = 0; index < records.length; index += 1) {
    const record = records[index];
    if (!record) continue;
    const code = record.slice(0, 2);
    const renamed = code.includes('R') || code.includes('C');
    const previousPath = renamed ? relativeToWorkspace(records[index + 1]!) : undefined;
    files.push({
      path: relativeToWorkspace(record.slice(3)),
      previousPath,
      code,
      staged: code[0] !== ' ' && code[0] !== '?',
      unstaged: code[1] !== ' ' && code[1] !== '?',
      untracked: code === '??',
    });
    if (renamed) index += 1;
  }
  return files;
}

export async function readGitSnapshot(cwd: string): Promise<GitSnapshot> {
  try {
    const [branchValue, status, prefixValue] = await Promise.all([
      runGit(cwd, ['branch', '--show-current']),
      runGit(cwd, ['status', '--porcelain=v1', '-z', '--untracked-files=all', '--', '.']),
      runGit(cwd, ['rev-parse', '--show-prefix']),
    ]);
    const branch = branchValue.trim() || (await runGit(cwd, ['rev-parse', '--short', 'HEAD'])).trim();
    return {
      branch,
      files: parseStatus(status, prefixValue.trim()),
      loading: false,
    };
  } catch (error) {
    return {
      branch: '',
      files: [],
      loading: false,
      error: error instanceof Error ? error.message : String(error),
    };
  }
}

export async function readGitDiff(cwd: string, path: string): Promise<string> {
  const [staged, unstaged] = await Promise.all([
    runGit(cwd, ['diff', '--cached', '--no-ext-diff', '--unified=3', '--', path]).catch(() => ''),
    runGit(cwd, ['diff', '--no-ext-diff', '--unified=3', '--', path]).catch(() => ''),
  ]);
  const parts = [
    staged.trim() ? `# Staged\n${staged.trimEnd()}` : '',
    unstaged.trim() ? `# Working tree\n${unstaged.trimEnd()}` : '',
  ].filter(Boolean);
  return parts.join('\n\n') || `No textual diff for ${path}.`;
}

export interface GitComparison {
  before: string;
  after: string;
  patch: string;
}

export async function readGitComparison(cwd: string, file: GitFile): Promise<GitComparison> {
  const prefix = (await runGit(cwd, ['rev-parse', '--show-prefix'])).trim();
  const headPath = posix.normalize(posix.join(prefix, file.previousPath ?? file.path));
  const [before, after, patch] = await Promise.all([
    runGit(cwd, ['show', `HEAD:${headPath}`]).catch(() => ''),
    readFile(resolve(cwd, file.path), 'utf8').catch(() => ''),
    readGitDiff(cwd, file.path),
  ]);
  return {
    before: sanitizeTerminalText(before),
    after: sanitizeTerminalText(after.slice(0, MAX_GIT_OUTPUT)),
    patch: sanitizeTerminalText(patch.slice(0, MAX_GIT_OUTPUT)),
  };
}
