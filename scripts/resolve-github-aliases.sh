#!/usr/bin/env bash
# Resolve GitHub usernames -> Amazon aliases via the puzzleglue open-source tool.
#
# puzzleglue maps a GitHub-connected employee's GH username to their Amazon alias:
#   GET https://puzzleglue.open-source.a2z.com/github/inspect?user=<ghUser>
#   -> {"id","alias","username","last_cached"}
#
# Auth: Midway. Run `mwinit` first; this uses the cookie at ~/.midway/cookie.
#
# Usage:
#   scripts/resolve-github-aliases.sh user1 user2 ...        # explicit usernames
#   scripts/resolve-github-aliases.sh --repo kiro-team/kiro-cli   # all repo contributors
#   printf 'user1\nuser2\n' | scripts/resolve-github-aliases.sh -   # stdin (one per line)
#
# Flags:
#   --repo <owner/repo>   Pull contributors via `gh` instead of taking args.
#   --release <tag>       Resolve the contributors OF a GitHub release (e.g. v2.16.1,
#                         or "latest"). Parses @handles from the release notes.
#   --include-core        Do NOT exclude kiro-cli core team (default: exclude).
#   --no-cache            Skip the on-disk cache (always hit puzzleglue, no write-back).
#   --json                Emit a JSON array instead of TSV.
#   -                     Read usernames from stdin.
#
# A resolved github->alias cache is kept in the repo at
# .kiro/skills/github-alias-resolver/alias-cache.json and grows as you resolve new
# users. Cache hits avoid a puzzleglue round-trip (and work offline / without Midway).
#
# Output (TSV default): github_username <tab> alias <tab> status
#   status = ok | cached | not-found | core-excluded
set -euo pipefail

ENDPOINT="https://puzzleglue.open-source.a2z.com/github/inspect"
COOKIE="${HOME}/.midway/cookie"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CORE_TEAM_JSON="${SCRIPT_DIR}/../.kiro/skills/community-contribution-report/core_team.json"
CACHE_FILE="${SCRIPT_DIR}/../.kiro/skills/github-alias-resolver/alias-cache.json"

EXCLUDE_CORE=1
OUTPUT="tsv"
USE_CACHE=1
REPO=""
RELEASE=""
DEFAULT_REPO="kiro-team/kiro-cli"
USERS=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --repo) REPO="$2"; shift 2 ;;
    --release) RELEASE="$2"; shift 2 ;;
    --include-core) EXCLUDE_CORE=0; shift ;;
    --no-cache) USE_CACHE=0; shift ;;
    --json) OUTPUT="json"; shift ;;
    -) mapfile -t stdin_users; USERS+=("${stdin_users[@]}"); shift ;;
    --help|-h) sed -n '2,36p' "$0"; exit 0 ;;
    *) USERS+=("$1"); shift ;;
  esac
done

command -v jq >/dev/null || { echo "error: jq is required" >&2; exit 1; }

# Collect usernames from --repo if given.
if [[ -n "$REPO" ]]; then
  command -v gh >/dev/null || { echo "error: gh is required for --repo" >&2; exit 1; }
  mapfile -t repo_users < <(gh api "repos/${REPO}/contributors" --paginate -q '.[].login')
  USERS+=("${repo_users[@]}")
fi

# Collect contributors OF a release by parsing "@handle" from its release notes.
if [[ -n "$RELEASE" ]]; then
  command -v gh >/dev/null || { echo "error: gh is required for --release" >&2; exit 1; }
  rel_repo="${REPO:-$DEFAULT_REPO}"
  if [[ "$RELEASE" == "latest" ]]; then
    rel_path="repos/${rel_repo}/releases/latest"
  else
    rel_path="repos/${rel_repo}/releases/tags/${RELEASE}"
  fi
  body="$(gh api "$rel_path" -q '.body' 2>/dev/null)" \
    || { echo "error: release '${RELEASE}' not found in ${rel_repo}" >&2; exit 1; }
  mapfile -t rel_users < <(grep -oE 'by @[A-Za-z0-9-]+' <<<"$body" \
    | sed 's/^by @//' | sort -u)
  [[ ${#rel_users[@]} -gt 0 ]] || echo "warn: no @contributors found in ${RELEASE} notes" >&2
  USERS+=("${rel_users[@]}")
fi

[[ ${#USERS[@]} -gt 0 ]] || { echo "error: no usernames provided" >&2; exit 1; }

# Build core-team exclusion set (case-insensitive).
declare -A CORE=()
if [[ "$EXCLUDE_CORE" -eq 1 && -f "$CORE_TEAM_JSON" ]]; then
  while IFS= read -r h; do CORE["${h,,}"]=1; done \
    < <(jq -r '.handles[]' "$CORE_TEAM_JSON")
fi

# Load the on-disk cache (github_username -> alias) into an assoc array.
declare -A CACHE_MAP=()
if [[ "$USE_CACHE" -eq 1 && -f "$CACHE_FILE" ]]; then
  while IFS=$'\t' read -r k v; do CACHE_MAP["$k"]="$v"; done \
    < <(jq -r '.aliases | to_entries[] | [.key, .value] | @tsv' "$CACHE_FILE" 2>/dev/null)
fi

# Lazy Midway auth check — only runs the first time we actually hit puzzleglue, so a
# fully-cached run needs no Midway. Authed responses are JSON (200 hit or 404 miss);
# an unauthenticated request redirects to Midway and returns HTML.
AUTH_CHECKED=0
ensure_auth() {
  [[ "$AUTH_CHECKED" -eq 1 ]] && return 0
  local probe
  probe="$(curl -sL --cookie "$COOKIE" --cookie-jar "$COOKIE" \
    "${ENDPOINT}?user=octocat" 2>/dev/null || true)"
  if ! jq -e . >/dev/null 2>&1 <<<"$probe"; then
    echo "error: puzzleglue auth failed. Run 'mwinit' (or 'mwinit -o' on macOS) and retry." >&2
    exit 2
  fi
  AUTH_CHECKED=1
}

resolve() {
  local u="$1"
  curl -sf -L --cookie "$COOKIE" --cookie-jar "$COOKIE" \
    "${ENDPOINT}?user=${u}" 2>/dev/null || echo ""
}

results="[]"
cache_dirty=0
for u in "${USERS[@]}"; do
  [[ -z "$u" ]] && continue
  if [[ "$EXCLUDE_CORE" -eq 1 && -n "${CORE[${u,,}]:-}" ]]; then
    row="$(jq -n --arg u "$u" '{github:$u,alias:null,status:"core-excluded"}')"
  elif [[ "$USE_CACHE" -eq 1 && -n "${CACHE_MAP[$u]:-}" ]]; then
    row="$(jq -n --arg u "$u" --arg a "${CACHE_MAP[$u]}" '{github:$u,alias:$a,status:"cached"}')"
  else
    ensure_auth
    body="$(resolve "$u")"
    alias="$(jq -r '.alias // empty' <<<"$body" 2>/dev/null || true)"
    if [[ -n "$alias" ]]; then
      row="$(jq -n --arg u "$u" --arg a "$alias" '{github:$u,alias:$a,status:"ok"}')"
      if [[ "$USE_CACHE" -eq 1 ]]; then CACHE_MAP["$u"]="$alias"; cache_dirty=1; fi
    else
      row="$(jq -n --arg u "$u" '{github:$u,alias:null,status:"not-found"}')"
    fi
  fi
  results="$(jq --argjson r "$row" '. + [$r]' <<<"$results")"
done

# Write the cache back (sorted by key) if we learned anything new.
if [[ "$USE_CACHE" -eq 1 && "$cache_dirty" -eq 1 ]]; then
  mkdir -p "$(dirname "$CACHE_FILE")"
  for k in "${!CACHE_MAP[@]}"; do printf '%s\t%s\n' "$k" "${CACHE_MAP[$k]}"; done \
    | sort \
    | jq -R -s --arg d "$(date -u +%Y-%m-%dT%H:%M:%SZ)" '
        (split("\n") | map(select(length>0) | split("\t") | {(.[0]): .[1]}) | add)
        as $aliases
        | { _description: "Cache of GitHub username -> Amazon alias, resolved via puzzleglue. Auto-maintained by scripts/resolve-github-aliases.sh.",
            _updated: $d,
            aliases: $aliases }' \
    > "${CACHE_FILE}.tmp"
  mv "${CACHE_FILE}.tmp" "$CACHE_FILE"
fi

if [[ "$OUTPUT" == "json" ]]; then
  jq '.' <<<"$results"
else
  jq -r '.[] | [.github, (.alias // "-"), .status] | @tsv' <<<"$results"
fi
