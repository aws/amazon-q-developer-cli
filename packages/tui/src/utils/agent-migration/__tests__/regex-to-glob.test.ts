/**
 * Unit tests for `regexToGlob` — the V2-regex → V3-glob translator.
 *
 * Grouped by the 3-way fidelity classification it returns:
 *  - lossless      — literals, anchors, `.*`, escaped literals, the
 *                    KAS-redundant no-chaining guard, alternation/optionals
 *  - lossy         — content char classes, bare `.`, `\d\w\s`, case-insensitive
 *  - unconvertible — chaining/redirection grammars, lookarounds, blow-ups
 */

import { describe, test, expect } from 'bun:test';

import { regexToGlob } from '../regex-to-glob.js';

const shell = (p: string) => regexToGlob(p, { dropChaining: true });
const web = (p: string) => regexToGlob(p, { dropChaining: false });
const sorted = (r: { globs: string[] }) => [...r.globs].sort();

describe('regexToGlob — lossless', () => {
  test('plain literal is returned unchanged', () => {
    expect(shell('brazil-build')).toEqual({
      globs: ['brazil-build'],
      fidelity: 'lossless',
    });
  });

  test('anchored literal drops the anchors', () => {
    expect(shell('^git status$')).toEqual({
      globs: ['git status'],
      fidelity: 'lossless',
    });
  });

  test('.* / .+ become *', () => {
    expect(shell('git diff.*')).toEqual({
      globs: ['git diff*'],
      fidelity: 'lossless',
    });
    expect(shell('.*rm -rf.*')).toEqual({
      globs: ['*rm -rf*'],
      fidelity: 'lossless',
    });
  });

  test('escaped literals stay literal', () => {
    expect(shell('^cargo \\+nightly fmt[^&;]*$')).toEqual({
      globs: ['cargo +nightly fmt*'],
      fidelity: 'lossless',
    });
  });

  test('no-shell-chaining guard [^&;|]* is redundant in KAS', () => {
    expect(shell('^git diff[^&;|]*$')).toEqual({
      globs: ['git diff*'],
      fidelity: 'lossless',
    });
  });

  test('alternation expands into one glob per branch', () => {
    const r = shell('cargo (build|test|check).*');
    expect(r.fidelity).toBe('lossless');
    expect(sorted(r)).toEqual(['cargo build*', 'cargo check*', 'cargo test*']);
  });

  test('non-capturing group (?:…) expands like a plain group', () => {
    const r = shell('git (?:status|log).*');
    expect(r.fidelity).toBe('lossless');
    expect(sorted(r)).toEqual(['git log*', 'git status*']);
  });

  test('optional group expands into with/without branches', () => {
    const r = shell('grep( .*)?');
    expect(r.fidelity).toBe('lossless');
    expect(sorted(r)).toEqual(['grep', 'grep *']);
  });

  test('optional chaining prefix keeps the standalone command', () => {
    const r = shell('^(cd [^ ]+ && )?(git status)( [^;|&`$]+)?$');
    expect(r.fidelity).toBe('lossless');
    expect(sorted(r)).toEqual(['git status', 'git status *']);
  });
});

describe('regexToGlob — lossy', () => {
  test('positive char class + quantifier broadens to *', () => {
    expect(shell('sleep [0-9]+')).toEqual({
      globs: ['sleep *'],
      fidelity: 'lossy',
    });
  });

  test('flag character class broadens to *', () => {
    expect(shell('grep -[rniElw]* .*')).toEqual({
      globs: ['grep -* *'],
      fidelity: 'lossy',
    });
  });

  test('bare . broadens to *', () => {
    expect(shell('ls .')).toEqual({ globs: ['ls *'], fidelity: 'lossy' });
  });

  test('\\d shorthand broadens to *', () => {
    expect(shell('sleep \\d+')).toEqual({
      globs: ['sleep *'],
      fidelity: 'lossy',
    });
  });

  test('leading (?i) flag is lossy (case-insensitivity has no glob)', () => {
    expect(shell('(?i)^make$')).toEqual({ globs: ['make'], fidelity: 'lossy' });
  });
});

describe('regexToGlob — unconvertible', () => {
  test('lookahead cannot be represented', () => {
    expect(shell('^git (?!push).*$')).toEqual({
      globs: [],
      fidelity: 'unconvertible',
    });
  });

  test('mandatory command chaining has no single-command glob', () => {
    expect(shell('^foo && bar$')).toEqual({
      globs: [],
      fidelity: 'unconvertible',
    });
  });

  test('a pipe-to-grep grammar with no chaining-free branch is unconvertible', () => {
    expect(shell('^cat [^;|&`$]+ \\| grep [^;|&`$]+$')).toEqual({
      globs: [],
      fidelity: 'unconvertible',
    });
  });

  test('monster full-command grammar (AIM template) is unconvertible', () => {
    const p =
      '(?s)^git (-P |--no-pager )*(status|log|diff|show)( [^;|&`$]+)?( 2>(&1|/dev/null))?( \\| (tail|head)( -n [0-9]+)?| \\| grep( [^;&`$]+)?)?$';
    expect(shell(p)).toEqual({ globs: [], fidelity: 'unconvertible' });
  });
});

describe('regexToGlob — web_fetch', () => {
  test('escaped-dot domain pattern converts faithfully', () => {
    expect(web('.*docs\\.aws\\.amazon\\.com.*')).toEqual({
      globs: ['*docs.aws.amazon.com*'],
      fidelity: 'lossless',
    });
  });

  test('web keeps & (query params are not chaining)', () => {
    expect(web('.*example\\.com/x\\?a=1&b=2.*')).toEqual({
      globs: ['*example.com/x?a=1&b=2*'],
      fidelity: 'lossless',
    });
  });
});

describe('regexToGlob — literal/space `*` quantifier is lossy, not lossless', () => {
  // `git *` is a REGEX ("git" + zero-or-more spaces), so the glob `git *`
  // ("git " + anything) does not match the same set — it must be flagged lossy.
  test('space-quantifier `cmd *` passes through but is lossy', () => {
    expect(shell('git *')).toEqual({ globs: ['git *'], fidelity: 'lossy' });
    expect(shell('ls *')).toEqual({ globs: ['ls *'], fidelity: 'lossy' });
  });

  test('literal-quantifier `cmd*` / `name*` is lossy', () => {
    expect(shell('brazil-build*')).toEqual({
      globs: ['brazil-build*'],
      fidelity: 'lossy',
    });
  });

  // Guard against over-flagging: a dot-star (`cmd .*`) is a genuine wildcard
  // and stays lossless — only bare `*` quantifiers on a literal are lossy.
  test('`cmd .*` (dot-star) stays lossless', () => {
    expect(shell('cat .*')).toEqual({ globs: ['cat *'], fidelity: 'lossless' });
    expect(shell('git diff.*')).toEqual({
      globs: ['git diff*'],
      fidelity: 'lossless',
    });
  });
});

describe('regexToGlob — safety', () => {
  test('never leaks raw regex syntax into an emitted glob', () => {
    const patterns = [
      'cargo (build|test).*',
      'grep( .*)?',
      '^(a|b|c)+$',
      'x{2,3}',
      '(?s)^git (status|log)( [^;|&`$]+)?$',
    ];
    for (const p of patterns) {
      for (const g of regexToGlob(p, { dropChaining: true }).globs) {
        expect(g).not.toMatch(/[()|\\^$]/);
      }
    }
  });
});
