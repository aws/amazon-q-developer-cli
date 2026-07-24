/**
 * Parser for the `_meta.kiro.repositories` array KAS reports on the ACP wire —
 * on `session/load` / `session/new` responses and on the `session_info_update`
 * notification the sandbox emits mid-session. A single parser ensures all
 * ingestion seams apply identical validation and null-vs-empty semantics.
 */

/** A session-bound repository as the footer needs it. */
export interface SessionRepositoryEntry {
  name: string;
  branch?: string;
}

/**
 * Parses an untrusted `_meta.kiro.repositories` value into `{name, branch?}`
 * entries. Entries missing a usable `name` are dropped. A non-array yields
 * `null` — "KAS didn't report" — which callers must treat differently from
 * `[]` ("the session has zero repos"): an absent report falls back to
 * client-side state, while an explicit empty report is authoritative.
 */
export function parseSessionRepositories(
  raw: unknown
): SessionRepositoryEntry[] | null {
  if (!Array.isArray(raw)) return null;
  const repos: SessionRepositoryEntry[] = [];
  for (const entry of raw) {
    if (typeof entry !== 'object' || entry === null) continue;
    const record = entry as Record<string, unknown>;
    const name = typeof record.name === 'string' ? record.name.trim() : '';
    if (!name) continue;
    const branch =
      typeof record.branch === 'string' && record.branch.trim().length > 0
        ? record.branch.trim()
        : undefined;
    repos.push({ name, ...(branch ? { branch } : {}) });
  }
  // Shape drift guard: raw contained entries but none parsed successfully.
  // Treat as "not reported" so callers fall back to client-side state instead
  // of authoritatively wiping the footer.
  if (raw.length > 0 && repos.length === 0) return null;
  return repos;
}
