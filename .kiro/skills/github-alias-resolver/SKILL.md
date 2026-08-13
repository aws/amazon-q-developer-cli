---
name: github-alias-resolver
description: Resolve GitHub usernames to Amazon aliases via the puzzleglue open-source tool, and find the Amazon aliases of the contributors to a given GitHub release. Use when asked to map GitHub contributors to Amazon aliases, identify who an external contributor is, find the Amazon alias behind a GitHub handle, build an alias list of non-core-team contributors, or answer natural-language questions like "who are the contributors for v2.16.1", "where are the contributors for <release/build>", "which aliases contributed to the latest release", or "who shipped in <version>". Triggers on "github username to alias", "who is <ghuser>", "map contributors to aliases", "external contributor aliases", "contributors for <release>", "contributors in <version>", "who contributed to <build>".
---

# GitHub → Amazon Alias Resolver

Maps a GitHub username to the Amazon alias of the GitHub-connected employee, using the
**puzzleglue** open-source tooling service. Useful for turning a list of GitHub
contributors into Amazon aliases — e.g. to identify external (non-core-team)
contributors to `kiro-team/kiro-cli`.

## Data source

`GET https://puzzleglue.open-source.a2z.com/github/inspect?user=<githubUser>`

Returns JSON:

```json
{ "id": "MDQ6VXNlcjQwMzAzMzQ=", "alias": "kennvene", "username": "kensave", "last_cached": "2026-08-06T20:24:41.677Z" }
```

- `alias` is the Amazon login. `404 {"error":"Item not found"}` means the GitHub user
  is not linked to an Amazon employee (external / not yet indexed).
- `last_cached` — results are cached, so a brand-new GitHub account may not resolve
  until puzzleglue re-indexes.

## Auth — Midway

puzzleglue is behind **Midway**. Authenticate once before running:

```bash
mwinit          # or `mwinit -o` on macOS
```

The script reads the cookie at `~/.midway/cookie`. If the cookie is missing/expired the
request redirects to Midway and returns HTML instead of JSON — the script detects this
and exits with `error: puzzleglue auth failed`.

## Usage

Script: `scripts/resolve-github-aliases.sh` (requires `jq`; `gh` only for `--repo`).

```bash
# Explicit usernames -> TSV (github <tab> alias <tab> status)
scripts/resolve-github-aliases.sh rubencu jguoamz rschmitt

# Contributors OF a release (parses @handles from the release notes)
scripts/resolve-github-aliases.sh --release v2.16.1     # a specific tag
scripts/resolve-github-aliases.sh --release latest      # newest release

# All contributors to a repo, excluding the core team (default)
scripts/resolve-github-aliases.sh --repo kiro-team/kiro-cli

# From stdin, one per line
gh pr list --repo kiro-team/kiro-cli --json author -q '.[].author.login' \
  | sort -u | scripts/resolve-github-aliases.sh -

# JSON output
scripts/resolve-github-aliases.sh --json rubencu jguoamz
```

## Natural-language questions

Map the question to a flag, then run the script and present the `alias` column:

| The user asks… | Run |
|----------------|-----|
| "who are the contributors for v2.16.1" / "where are the contributors for that build" | `--release v2.16.1` |
| "which aliases shipped in the latest release" | `--release latest` |
| "who is `<ghuser>`" / "what's the alias for `<ghuser>`" | `<ghuser>` |
| "map all external contributors to aliases" | `--repo kiro-team/kiro-cli` |

- A "build"/"version"/"release" like `v2.16.1` (or a bare `2.16.1` → prefix `v`) is a
  release **tag** → use `--release`. "latest"/"most recent release" → `--release latest`.
- Releases default to `kiro-team/kiro-cli`; combine with `--repo` for another repo.
- Default excludes the core team (answering "who are the *community* contributors").
  Add `--include-core` if the user wants everyone.

Flags:

| Flag | Effect |
|------|--------|
| `--repo <owner/repo>` | Pull contributors via `gh api repos/<repo>/contributors` instead of args |
| `--release <tag>` | Resolve the contributors of a release (tag or `latest`); parses `@handles` from the release notes |
| `--include-core` | Do **not** exclude the core team (default is to exclude) |
| `--no-cache` | Skip the on-disk cache — always hit puzzleglue, no write-back |
| `--json` | Emit a JSON array instead of TSV |
| `-` | Read usernames from stdin (one per line) |

`status` column: `ok` (freshly resolved) | `cached` (from disk) | `not-found` | `core-excluded`.

## Cache

Resolved `github → alias` pairs are cached in-repo at
`.kiro/skills/github-alias-resolver/alias-cache.json` and the file grows as you resolve
new users. Benefits:

- Cache hits skip the puzzleglue round-trip — **and need no Midway**, so a fully-cached
  run works offline.
- Committing the cache means the mapping is shared across the team and keeps improving.

The cache stores only successful resolutions (not `not-found` or `core-excluded`), is
written back sorted by key, and is safe to edit or delete by hand (it regenerates).
Pass `--no-cache` to force a fresh lookup (e.g. if someone's alias changed).

## Core-team exclusion

The exclusion set is the shared `core_team.json` used by the
`community-contribution-report` skill:
`.kiro/skills/community-contribution-report/core_team.json`. Matching is
case-insensitive. **Update that file** (not this skill) when engineers join/leave the
team.

## When to update

- puzzleglue endpoint or response shape changes.
- Auth mechanism changes (e.g. away from Midway cookie).
