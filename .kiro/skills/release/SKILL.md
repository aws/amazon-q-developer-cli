---
name: release
description: Kiro CLI release SOP. Use when cutting a release, building RC/stable, promoting to prod, or troubleshooting release workflows. Triggers on "release", "cut release", "promote to prod", "build stable", "build RC".
disable-model-invocation: true
---

# Kiro CLI Release SOP (Tag-Driven)

> **Recommended agent:** `oncall`. This skill needs `TicketingReadActions`, `TicketingWriteActions`, `gh` shell access, and internet/code search. If you're not on the `oncall` agent, suggest the user switch with `/agent swap oncall` before continuing.

## Current State

!`echo "=== Last 5 nightlies ===" && gh api repos/kiro-team/kiro-cli/git/refs/tags --jq '.[].ref' 2>/dev/null | sed 's|refs/tags/||' | grep "nightly" | sort -rV | head -5 && echo "" && echo "=== Latest RC ===" && gh api repos/kiro-team/kiro-cli/git/refs/tags --jq '.[].ref' 2>/dev/null | sed 's|refs/tags/||' | grep "\-rc\." | sort -rV | head -1 && echo "" && echo "=== Latest stable ===" && gh api repos/kiro-team/kiro-cli/git/refs/tags --jq '.[].ref' 2>/dev/null | sed 's|refs/tags/||' | grep -E "^v[0-9]+\.[0-9]+\.[0-9]+$" | sort -rV | head -1 && echo "" && echo "=== Release branches ===" && gh api repos/kiro-team/kiro-cli/branches --paginate --jq '.[].name' 2>/dev/null | grep -E "^release/[0-9]+\.[0-9]+\.[0-9]+$" | sort -rV`

## Tracking Ticket

Each release has a tracking ticket that persists state across sessions. Search for it first:

```
search-tickets: title:"Kiro CLI <version>" status:["Open","Assigned","Pending","Researching","Work In Progress"]
```

If found, read the **WORKLOG** thread — the latest comment contains a JSON state block. Use it to determine the resume point.

If not found, create one (only when starting at Step 1):

```
create-ticket:
  title: "Kiro CLI <version>"
  description: "Tracking Release Ticket"
  severity: SEV_5
  categorization: [{key: category, value: Kiro}, {key: type, value: CLI}, {key: item, value: Intake}]
  assignedGroup: "Amazon Q for CLI"
```

### State Schema

After every step, append a comment to **WORKLOG** with the updated state JSON:

```json
{
  "version": "2.6.0",
  "branch": "release/2.6.0",
  "base_tag": "v2.5.0-nightly.13",
  "last_step": 5,
  "rc_tag": "v2.6.0-rc.2",
  "stable_tag": "v2.6.0",
  "chat_commit": null,
  "autocomplete_commit": null,
  "promoted_to_toolbox": false,
  "promoted_to_cloudfront": false
}
```

Append a human-readable progress comment to **CORRESPONDENCE** with run links, diagnostics output, etc.

When Step 8 succeeds, resolve the ticket.

## Release Type: Standard vs Hotfix

**Before choosing a path, decide which release type fits.** Surface this question to the user explicitly — don't default silently.

| Release type | When to use | Path |
|--------------|-------------|------|
| **Standard** (Steps 1–8 below) | Minor/major releases. Patch releases that bundle multiple fixes, or where you want a public RC for broader verification. | RC build → verify RC → stable build → verify stable → promote |
| **Hotfix** (skips RC) | A small, isolated, well-understood fix on top of an already-shipped stable version — typically: <br>• A single commit (or a few tightly related commits) <br>• Build/CI/config-only or a clearly scoped runtime fix <br>• Customer-blocking, where the ~60–90 min RC cycle is meaningful overhead | Cherry-pick → stable build → verify on beta → promote. See [Hotfix Process](reference.md#hotfix-process). |

**Default:** when in doubt, ask. If the change is a single isolated commit and there's an open customer-blocking ticket, propose Hotfix. Otherwise propose Standard.

Patch releases (`X.Y.Z+1`) can use *either* path — "patch" describes the version number, not the process. The deciding factor is the size and risk of the change.

## Release Steps

Based on Current State and the latest WORKLOG state (if a tracking ticket exists), identify the resume point:

| State | Possible next actions |
|-------|----------------------|
| No tracking ticket, no `release/X.Y.Z` branch | Step 1: Cut new release |
| Branch exists, no RC tag (Standard) | Step 3: Build RC |
| Branch exists, no stable tag (Hotfix) | Step 5: Build stable |
| RC tag exists | Step 3: Build another RC, or Step 5: Build stable |
| Stable tag exists | Step 7: Promote to prod, or already done |

Ask the user for:
1. **Target version** (e.g., `2.6.0`)
2. **Base tag** to branch from — a nightly (e.g., `v2.5.0-nightly.5`) for new releases, or a stable tag (e.g., `v2.5.0`) for patch releases.
3. **Release type** — Standard (full RC + stable cycle) or Hotfix (skip RC). See above.

Confirm with the user before each major step.

> **Hotfix shortcut:** If the user chose Hotfix, run Steps 1, 2, then **skip directly to Step 5** (build stable) → Step 6 → Step 7 → Step 8.

### Step 1: Cut Release Branch

If a `release/<version>` branch already exists (shown in Current State), skip this step.

Validate the base_tag is a nightly or stable tag (not RC) before running.

```bash
# Chat repo
gh workflow run cut-release.yml --ref main -f version=<version> -f base_tag=<base_tag>

# Autocomplete repo
gh workflow run create-release-branch.yml \
  --repo kiro-team/kiro-cli-autocomplete \
  --ref main \
  -f version=<version> \
  -f base_tag=<base_tag>
```

**Verify both branches exist before proceeding:**
```bash
gh api repos/kiro-team/kiro-cli/branches/release/<version> --jq '.name'
gh api repos/kiro-team/kiro-cli-autocomplete/branches/release/<version> --jq '.name'
```

### Step 2: Cherry-pick & Changelog

- Cherry-pick fixes from `main` to `release/<version>` via PRs (both repos)
- Check for reverts on `main` that need to be applied to the release branch:
  ```bash
  git log --oneline origin/release/<version>..origin/main | grep -i "revert"
  ```
- Update `feed.json` (see below)

#### Updating feed.json

Feed file: `crates/chat-cli/src/cli/feed.json` (chat-cli is the sole owner; the TUI and legacy chat both read this copy)

**For a minor release** (nightly base tag):

1. Identify new fragments added since the last stable release:
   ```bash
   git diff <last_stable_tag>..<base_tag> --name-only --diff-filter=A -- '.changes/*.json'
   ```
2. Only these fragments go into the new feed.json entry — not every fragment in `.changes/`.
   (`.changes/` on main accumulates fragments across releases and is never cleaned; releases
   are cut from tags, so the diff between tags defines a release's changes.)
   Read each fragment and assemble one release entry:
   ```json
   {
     "type": "release",
     "date": "<today, YYYY-MM-DD>",
     "version": "<version>",
     "title": "Version <version>",
     "changes": [ { "type": "<fragment type>", "description": "<fragment description>" } ]
   }
   ```
3. Insert the new entry into `crates/chat-cli/src/cli/feed.json` via text-level insertion
   immediately after the `0.0.0` `hidden` placeholder entry (`entries[0]`).
   Do NOT re-serialize the entire file — this changes unicode escapes and formatting in old entries.
4. Submit as a PR against `release/<version>` (branch protection requires PRs).
5. **Sync feed.json to autocomplete repo**: The autocomplete repo has its own `feed.json` at the repo root. Copy the updated feed.json from `crates/chat-cli/src/cli/feed.json` to the autocomplete repo's `release/<version>` branch:
   ```bash
   cd <autocomplete-worktree>
   cp <kiro-cli-worktree>/crates/chat-cli/src/cli/feed.json ./feed.json
   git add feed.json && git commit -m "fix: sync feed.json with kiro-cli" && git push
   ```

**For a patch release** (stable base tag):

1. Patch releases have 1–2 cherry-picked fixes. Changelog entries are written by hand
   or included as fragments alongside the cherry-pick PR.
2. The same text-level insertion and PR process applies.

### Step 3: Build RC

```bash
gh workflow run build-and-release.yml --ref release/<version> -f increment=rc
```

### Step 4: Verify RC

```bash
toolbox install kiro-cli --channel=beta --force
kiro-cli diagnostics
kiro-cli-chat diagnostics
```

### Step 5: Build Stable

```bash
gh workflow run build-and-release.yml --ref release/<version> -f increment=stable
```

### Step 6: Verify Stable on Beta

```bash
toolbox install kiro-cli --channel=beta --force
kiro-cli diagnostics
kiro-cli-chat diagnostics
```

Confirm version shows `<version>` (no `-rc`). Note both `hash` values for Step 7.

### Step 7: Promote to Production

Use the commit SHAs from the Step 6 diagnostics output (`hash` field).

```bash
gh workflow run promote-to-prod.yml \
  --repo kiro-team/kiro-cli-autocomplete \
  --ref release/<version> \
  -f commit=<autocomplete_hash_from_diagnostics> \
  -f chat_commit=<chat_hash_from_diagnostics>
```

Flow:
1. Approve → releases to prod toolbox
2. 15-min bake (automatic)
3. Approve → releases to CloudFront

### Step 8: Verify Production

```bash
# Toolbox
toolbox install kiro-cli --force
kiro-cli diagnostics

# CloudFront
curl -fsSL https://cli.kiro.dev/install | bash
kiro-cli --version
```

## Troubleshooting

See [reference.md](reference.md) for known issues, workflow reference, environments, hotfix, and rollback.
