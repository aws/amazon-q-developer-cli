/**
 * Translate a single V2 (CLI) regex into a V3 (KAS) Cedar-`like` glob (only `*`
 * is special). Classified `lossless` / `lossy` (broadened) / `unconvertible`
 * (no safe glob — emit nothing rather than fabricate an over-broad allow); one
 * regex may fan out to several globs.
 *
 * KAS splits a command on shell operators and evaluates each sub-command
 * independently, so the "no shell-chaining" guard is redundant (→ `*`
 * losslessly) and a still-chaining branch is dead for shell.
 */

export type RegexFidelity = 'lossless' | 'lossy' | 'unconvertible';

export interface RegexToGlobResult {
  /** Emittable globs; empty iff `fidelity === 'unconvertible'`. */
  globs: string[];
  fidelity: RegexFidelity;
}

const REGEX_METACHARS = /[.*+?^${}()|[\]\\]/;
const LOOKAROUND = /\(\?<?[=!]/;
// Shell chaining operators — a glob containing one can't match a single command.
const SHELL_CHAINING = /[|;&]/;
const MAX_BRANCHES = 256;

// Sentinels shielding escaped literals mid-conversion, and the (constant)
// regex that later unshields them.
const PROT_OPEN = '\u0000';
const PROT_CLOSE = '\u0001';
const PROT_RE = new RegExp(`${PROT_OPEN}(\\d+)${PROT_CLOSE}`, 'g');

/**
 * `dropChaining` (shell): discard expanded branches that still chain — dead
 * under KAS tokenization. False for web URLs, where `&`/`;` are legitimate.
 */
export function regexToGlob(
  pattern: string,
  options: { dropChaining?: boolean } = {}
): RegexToGlobResult {
  if (!REGEX_METACHARS.test(pattern)) {
    return { globs: [pattern], fidelity: 'lossless' };
  }

  // Lookaround / lookbehind can't be represented as a glob.
  if (LOOKAROUND.test(pattern)) {
    return { globs: [], fidelity: 'unconvertible' };
  }

  let body = pattern;

  // Strip a leading inline-flag group (`(?i)` etc.); case-insensitivity is lossy.
  let caseInsensitive = false;
  const flagGroup = body.match(/^\(\?([a-z]+)\)/);
  if (flagGroup) {
    caseInsensitive = flagGroup[1]!.includes('i');
    body = body.slice(flagGroup[0].length);
  }

  // Non-capturing groups expand exactly like plain groups.
  body = body.replace(/\(\?:/g, '(');

  if (body.startsWith('^')) {
    body = body.slice(1);
  }
  if (body.endsWith('$') && !body.endsWith('\\$')) {
    body = body.slice(0, -1);
  }

  let branches = expandGroups(body);
  const blewUp = branches.length > MAX_BRANCHES;
  if (blewUp) {
    branches = [body];
  }

  const converted = branches.map(branchToGlob);

  // For shell, prefer chaining-free branches; keep chaining ones only if
  // nothing else survives (so they can be flagged unconvertible below).
  let kept = converted;
  if (options.dropChaining) {
    const standalone = converted.filter((c) => !SHELL_CHAINING.test(c.glob));
    if (standalone.length > 0) {
      kept = standalone;
    }
  }

  const seen = new Set<string>();
  const globs: string[] = [];
  let lossy = caseInsensitive;
  for (const c of kept) {
    lossy = lossy || c.lossy;
    if (c.glob.length === 0 || seen.has(c.glob)) {
      continue;
    }
    seen.add(c.glob);
    globs.push(c.glob);
  }

  // Unconvertible when expansion blew up, produced nothing, or a surviving
  // glob still carries unsafe structure (chaining for shell, a leftover group
  // for either) — emitting it as an allow rule would be dangerously broad.
  const unsafe = (g: string) =>
    /[()]/.test(g) ||
    (options.dropChaining ? SHELL_CHAINING.test(g) : /\|/.test(g));
  if (blewUp || globs.length === 0 || globs.some(unsafe)) {
    return { globs: [], fidelity: 'unconvertible' };
  }

  return { globs, fidelity: lossy ? 'lossy' : 'lossless' };
}

/** Expand the first top-level `(…)` group and recurse, fanning alternation/optionals into a cross product; plain groups inline. */
function expandGroups(pattern: string): string[] {
  const open = pattern.indexOf('(');
  if (open === -1) {
    return [pattern];
  }

  let depth = 0;
  let close = -1;
  for (let i = open; i < pattern.length; i++) {
    const ch = pattern[i];
    if (ch === '\\') {
      i++;
      continue;
    }
    if (ch === '(') {
      depth++;
    } else if (ch === ')') {
      depth--;
      if (depth === 0) {
        close = i;
        break;
      }
    }
  }
  if (close === -1) {
    return [pattern]; // unbalanced — treat as opaque
  }

  const prefix = pattern.slice(0, open);
  const inner = pattern.slice(open + 1, close);
  let rest = pattern.slice(close + 1);

  let optional = false;
  if (rest.startsWith('?')) {
    optional = true;
    rest = rest.slice(1);
  }

  const alternatives = splitTopLevel(inner, '|');
  const options = optional ? [...alternatives, ''] : alternatives;

  const tails = expandGroups(rest);
  const results: string[] = [];
  for (const opt of options) {
    for (const mid of expandGroups(opt)) {
      for (const tail of tails) {
        results.push(prefix + mid + tail);
        if (results.length > MAX_BRANCHES) {
          return results;
        }
      }
    }
  }
  return results;
}

/** Split on `sep`, ignoring separators inside `(…)` / `[…]` or after `\`. */
function splitTopLevel(s: string, sep: string): string[] {
  const parts: string[] = [];
  let paren = 0;
  let klass = 0;
  let cur = '';
  for (let i = 0; i < s.length; i++) {
    const ch = s[i];
    if (ch === '\\') {
      cur += ch + (s[i + 1] ?? '');
      i++;
      continue;
    }
    if (ch === '[') {
      klass++;
    } else if (ch === ']') {
      if (klass > 0) klass--;
    } else if (ch === '(') {
      paren++;
    } else if (ch === ')') {
      if (paren > 0) paren--;
    } else if (ch === sep && paren === 0 && klass === 0) {
      parts.push(cur);
      cur = '';
      continue;
    }
    cur += ch;
  }
  parts.push(cur);
  return parts;
}

/** Convert one group-free branch to a glob, tracking whether it broadened. */
function branchToGlob(branch: string): { glob: string; lossy: boolean } {
  let lossy = false;
  let s = branch;

  s = s.replace(/\\[dwsDWS][*+]?/g, () => {
    lossy = true;
    return '*';
  });

  // Shield escaped literals so later steps don't reinterpret them as syntax.
  const literals: string[] = [];
  s = s.replace(/\\([\s\S])/g, (_m, ch: string) => {
    literals.push(ch);
    return `${PROT_OPEN}${literals.length - 1}${PROT_CLOSE}`;
  });

  // A regex `*` quantifier on a literal/space has no exact glob form; we pass
  // it through as a wildcard (usually intent), but it's lossy not lossless.
  if (/(^|[^.\])])\*/.test(s)) {
    lossy = true;
  }

  // Negated class → `*`: a "not a metacharacter" guard KAS enforces natively.
  s = s.replace(/\[\^[^\]]*\][*+]?/g, '*');

  s = s.replace(/\[[^\]]*\][*+]?/g, () => {
    lossy = true;
    return '*';
  });

  s = s.replace(/\.[*+]/g, '*');

  // Bare `.` broadens — Cedar `like` has no single-char wildcard.
  s = s.replace(/\./g, () => {
    lossy = true;
    return '*';
  });

  s = s.replace(/[$^]/g, '');

  // Catch-all so raw regex syntax never leaks into a glob.
  if (/[+?{}()|\\]/.test(s)) {
    s = s.replace(/\{[^}]*\}/g, '*').replace(/[+?()|\\]/g, '*');
    lossy = true;
  }

  s = s.replace(/\*{2,}/g, '*');

  s = s.replace(PROT_RE, (_m, idx: string) => literals[Number(idx)] ?? '');

  return { glob: s, lossy };
}
