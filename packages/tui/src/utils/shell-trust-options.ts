import type { ConsentContext } from '../types/agent-events.js';

/** The trust values an approval offers for a (possibly compound) shell command. */
export interface ShellTrustOptions {
  /**
   * The sub-command actually being gated this round. Compound shell commands
   * (e.g. `git status && echo "done"`) are gated one segment at a time by the
   * agent: the whole command arrives as `resource`, but the segment requiring
   * consent right now is `triggeringResource`. Falls back to `resource` for
   * single / non-compound requests (no `triggeringResource`).
   */
  gatedResource?: string;
  /**
   * Exact-match trust value to persist. Undefined when the gated resource is
   * the KAS whole-capability wildcard, because that cannot be represented as a
   * literal exact resource in the permission response.
   */
  exactResource?: string;
  /**
   * Pattern trust value (`<first token> *`, e.g. `echo *`) for shell-like
   * capabilities whose gated resource carries arguments; undefined otherwise.
   * Derived from the GATED resource, not the whole command, so trusting the
   * pattern actually authorizes the segment being asked about.
   */
  patternResource?: string;
}

/** Capabilities whose `resource` is a shell command string we can pattern on. */
export const KAS_WHOLE_CAPABILITY_RESOURCE = '*';

export function isKasShellCapability(capability?: string): boolean {
  return (
    capability === 'shell' ||
    capability === 'exec' ||
    capability === 'shell:exec'
  );
}

/**
 * First tokens that must never seed a `<token> *` trust pattern. These are
 * privilege/environment wrappers: trusting `sudo *` would authorize every
 * sudo-prefixed command (privilege-wide), and `env *` every env-prefixed one —
 * far broader than the single segment actually being gated. Matched
 * case-sensitively against the bare first token.
 */
const UNSAFE_PATTERN_COMMANDS = new Set(['sudo', 'doas', 'env']);

/**
 * Derive the trust-scope options offered for a v3/KAS shell approval. The exact
 * and pattern options both target the GATED sub-command (`triggeringResource`),
 * not the whole command, so that for a compound command like
 * `git status && echo "done"` whose gated segment is `echo "done"`, trusting
 * "exact" persists `echo "done"` and trusting "pattern" persists `echo *` —
 * either of which authorizes the segment. Using the whole command (the prior
 * behavior) offered only `git *` / the whole-command exact match, neither of
 * which matched the gated `echo "done"`, so the policy re-asked that segment
 * forever. Pure and unit-testable, independent of Ink rendering.
 */
export function deriveShellTrustOptions(
  input: Pick<ConsentContext, 'capability' | 'resource' | 'triggeringResource'>
): ShellTrustOptions {
  const gatedResource = input.triggeringResource ?? input.resource;
  let patternResource: string | undefined;
  if (gatedResource && isKasShellCapability(input.capability)) {
    // Tokenize once with a single consistent rule (trim first so leading
    // whitespace can't produce an empty first token / a `" *"` pattern).
    const tokens = gatedResource.trim().split(/\s+/);
    const tok = tokens[0] ?? '';
    // Only offer a `<command> *` pattern when the base command is safe and
    // meaningful. Now that compound inner segments are gated individually,
    // a naive first-token pattern can mint over-broad or misleading trust, so
    // skip these cases (leaving patternResource undefined):
    //   - empty token (whitespace-only input) → garbage `" *"` matching all
    //   - single bare token (no args) → `<cmd> *` is broader than the exact match
    //   - literal `*` → already a wildcard; `* *` is meaningless / over-broad
    //   - env-assignment prefix (`FOO=bar cmd`) → `FOO=bar *` is wrong, won't match
    //   - privilege/env wrappers (sudo|doas|env) → `sudo *` trusts ALL sudo commands
    // v2's command parser is danger-aware; this stays a minimal heuristic on
    // purpose rather than pulling in a full shell parser.
    if (
      tok !== '' &&
      tokens.length > 1 &&
      tok !== '*' &&
      !tok.includes('=') &&
      !UNSAFE_PATTERN_COMMANDS.has(tok)
    ) {
      patternResource = tok + ' *';
    }
  }
  return {
    gatedResource,
    exactResource:
      gatedResource === KAS_WHOLE_CAPABILITY_RESOURCE
        ? undefined
        : gatedResource,
    patternResource,
  };
}
