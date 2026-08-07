---
name: release
description: Kiro CLI release SOP. Use when cutting a release, building stable, promoting to prod, or troubleshooting release workflows. Triggers on "release", "cut release", "promote to prod", "build stable".
disable-model-invocation: true
---

# Kiro CLI Release SOP (Tag-Driven)

## STOP — Agent Check (Mandatory Pre-flight)

**Before doing anything else, verify the current agent is `oncall`.**

This skill requires tools that only the `oncall` agent has access to: `TicketingReadActions`, `TicketingWriteActions`, `gh` shell access via `execute_bash`, and internet/code search. Running from any other agent will fail partway through the release with cryptic tool-permission errors — potentially leaving the release in a half-cut state.

If you are **not** the `oncall` agent, respond with exactly this and then stop:

> This release skill must be run from the `oncall` agent. Please switch with `/agent swap oncall` and re-invoke the release skill. I will not proceed until then.

Do not attempt any release step, do not read further in this skill, do not offer workarounds. The switch is mandatory.

If you **are** the `oncall` agent, acknowledge briefly ("Running as `oncall` — proceeding.") and continue with the rest of this skill.

---


## Current State

!`echo "=== Last 2 nightlies ===" && gh api repos/kiro-team/kiro-cli/git/refs/tags --jq '.[].ref' 2>/dev/null | sed 's|refs/tags/||' | grep "nightly" | sort -rV | head -2 && echo "" && echo "=== Latest stable ===" && gh api repos/kiro-team/kiro-cli/git/refs/tags --jq '.[].ref' 2>/dev/null | sed 's|refs/tags/||' | grep -E "^v[0-9]+\.[0-9]+\.[0-9]+$" | sort -rV | head -1 && echo "" && echo "=== Release branches ===" && gh api repos/kiro-team/kiro-cli/branches --paginate --jq '.[].name' 2>/dev/null | grep -E "^release/[0-9]+\.[0-9]+\.[0-9]+$" | sort -rV`

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
  "version": "2.10.0",
  "branch": "release/2.10.0",
  "base_tag": "v2.9.0-nightly.5",
  "last_step": 3,
  "stable_tag": "v2.10.0",
  "chat_commit": null,
  "autocomplete_commit": null,
  "promoted_to_toolbox": false,
  "promoted_to_cloudfront": false
}
```

Append a human-readable progress comment to **CORRESPONDENCE** with run links, diagnostics output, etc.

When Step 6 succeeds, resolve the ticket.

## Determine Version

Before starting the release, compute the changelog to decide the version number.

1. Identify the latest stable tag (used to compute new fragments below — `base_tag` is asked in Step 1):
   ```bash
   LAST_STABLE=$(gh api repos/kiro-team/kiro-cli/git/refs/tags --jq '.[].ref' | sed 's|refs/tags/||' | grep -E '^v[0-9]+\.[0-9]+\.[0-9]+$' | sort -rV | head -1)
   echo "Last stable: $LAST_STABLE"
   ```

2. Compute new fragments since the last stable release:
   ```bash
   git diff "$LAST_STABLE"..HEAD --name-only --diff-filter=A -- '.changes/*.json'
   ```

3. Read each fragment and count entries by type:
   ```bash
   ADDED_COUNT=$(git diff "$LAST_STABLE"..HEAD --name-only --diff-filter=A -- '.changes/*.json' | xargs -I{} jq -r '.type' {} 2>/dev/null | grep -c '^added$')
   echo "Added entries: $ADDED_COUNT"
   ```

4. **Version suggestion (heuristic only — the operator makes the final call):**
   - `ADDED_COUNT >= 2` → suggest **minor** (`X.Y+1.0`)
   - otherwise → suggest **patch** (`X.Y.Z+1`)

   The count is only a starting signal. A single large/breaking/risky change can warrant a
   minor even with a low `added` count, and two trivial adds can stay a patch — so the count
   never decides on its own.

5. **Present to the operator and let them assess complexity — don't proceed on the heuristic alone.**
   Show the full fragment list (type + description) with the suggested bump, then ask the operator
   to confirm or override:
   > Heuristic suggests **`<minor|patch>`** → `<version>` (from `<N>` `added` fragments).
   > Full changelog (`<M>` fragments): `<type: description list>`
   > Does the complexity/risk match, or should we adjust? Please confirm the version.

6. **Ask the operator for the target deploy date.** Ask when they intend to deploy this release, as `YYYY-MM-DD`, and record it as `<deploy_date>`. It is used as the `date` field of the release entry when writing `feed.json` in Step 2 (instead of "today"). If they don't have a firm date yet, default to today and note that the feed.json date can be updated when Step 2 runs.

## Release Steps

Based on Current State and the latest WORKLOG state (if a tracking ticket exists), identify the resume point:

| State | Next step |
|-------|-----------|
| No tracking ticket, no `release/X.Y.Z` branch | Step 1: Cut new release |
| Branch exists, feed.json not yet updated | Step 2: Cherry-pick & feed.json |
| feed.json PRs merged, no stable build yet | Step 3: Build Stable |
| Stable build succeeded, not verified on beta | Step 4: Verify Stable on Beta |
| Verified on beta, not yet promoted | Step 5: Promote to Production |
| Promoted to prod toolbox + CloudFront, not verified | Step 6: Verify Production |
| All steps done | Release complete — resolve tracking ticket |

Confirm with the user before each major step.

### Step 1: Cut Release Branch

If a `release/<version>` branch already exists (shown in Current State), skip this step.

Ask the user for:
1. **Target version** (from the Determine Version section above)
2. **Base tag** to branch from — a nightly (e.g., `v2.9.0-nightly.5`) for new releases, or a stable tag (e.g., `v2.9.0`) for patch releases.

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

### Step 2: Cherry-pick & feed.json

- Cherry-pick fixes from `main` to `release/<version>` via PRs (both repos) if needed
- Check for reverts on `main` that need to be applied to the release branch:
  ```bash
  git log --oneline origin/release/<version>..origin/main | grep -i "revert"
  ```
- Update `feed.json` (see below)

#### Updating feed.json

Feed file: `crates/chat-cli/src/cli/feed.json`

This file is both compiled into the binary (fallback copy) and published as the hosted changelog feed the CLI fetches at runtime. Publishing is automatic: the stable build writes gamma's versioned and rolling objects (Step 3), and promote-to-prod writes only the prod versioned object (Step 5). For content fixes outside a release, use the `publish-changelog` skill.

The release branch's feed.json should contain **exactly two entries** (plus the hidden placeholder):
- The `0.0.0` hidden placeholder (`entries[0]`, always present)
- The **current** release entry (the one being shipped)
- The **previous stable** release entry (so `/changelog` can show "what changed since last time")

**Process:**

1. Get the previous stable release's feed.json entry from its release branch:
   ```bash
   git show origin/release/<previous_version>:crates/chat-cli/src/cli/feed.json
   ```
   Extract the entry for `<previous_version>` from that file.

2. Compute the current release's changelog from fragments:
   ```bash
   git diff <last_stable_tag>..<base_tag> --name-only --diff-filter=A -- '.changes/*.json'
   ```
   Read each fragment and assemble the new release entry:
   ```json
   {
     "type": "release",
     "date": "<deploy_date from Determine Version, YYYY-MM-DD>",
     "version": "<version>",
     "title": "Version <version>",
     "changes": [ { "type": "<fragment type>", "description": "<fragment description>" } ]
   }
   ```

3. Write `crates/chat-cli/src/cli/feed.json` with exactly:
   ```json
   {
     "$schema": "./feed-schema.json",
     "entries": [
       { "hidden placeholder (0.0.0)" },
       { "current version entry" },
       { "previous stable version entry" }
     ]
   }
   ```

4. **Submit as a PR against `release/<version>`** in the chat repo (branch protection requires PRs).

5. **Sync feed.json to autocomplete repo via PR**: The autocomplete repo has its own `feed.json` at the repo root. Copy the updated feed.json and submit as a PR against `release/<version>` in the autocomplete repo:
   ```bash
   cd <autocomplete-worktree>
   git checkout release/<version>
   cp <kiro-cli-worktree>/crates/chat-cli/src/cli/feed.json ./feed.json
   git checkout -b feed-json-<version>
   git add feed.json && git commit -m "chore: update feed.json for v<version>"
   git push -u origin feed-json-<version>
   gh pr create --repo kiro-team/kiro-cli-autocomplete \
     --base release/<version> --head feed-json-<version> \
     --title "chore: update feed.json for v<version>" \
     --body "Syncs feed.json from kiro-cli for the v<version> release."
   ```

### Step 3: Build Stable

```bash
gh workflow run build-and-release.yml --ref release/<version> -f increment=stable
```

The stable build also publishes the release branch's feed.json to both gamma paths. The versioned object validates the stable release's per-version content; nightly/rc/feature clients read the rolling object because feeds are not published for every prerelease version. Verify both after the build:
```bash
curl -s https://download.gamma.cli.kiro.dev/stable/<version>/feed.json | jq -r '.entries[].version'
curl -s https://download.gamma.cli.kiro.dev/stable/changelog/feed.json | jq -r '.entries[].version'
# expect <version> listed by both
```

### Step 4: Verify Stable on Beta

The stable build auto-releases to the beta toolbox channel.

```bash
toolbox install kiro-cli --channel=beta --force
kiro-cli diagnostics
kiro-cli-chat diagnostics
```

Confirm version shows `<version>`. Note both `hash` values for Step 5.

### Step 5: Promote to Production

Use the commit SHAs from the Step 4 diagnostics output (`hash` field).

```bash
gh workflow run promote-to-prod.yml \
  --repo kiro-team/kiro-cli-autocomplete \
  --ref release/<version> \
  -f commit=<autocomplete_hash_from_diagnostics> \
  -f chat_commit=<chat_hash_from_diagnostics>
```

Flow:
1. Publishes the release branch's feed.json to the versioned **prod** changelog path and waits for success (automatic `publish-changelog` job)
2. Approve → releases to prod toolbox
3. 15-min bake (automatic)
4. Approve → releases to CloudFront

### Step 6: Verify Production

```bash
# Toolbox
toolbox install kiro-cli --force
kiro-cli diagnostics

# CloudFront
curl -fsSL https://cli.kiro.dev/install | bash
kiro-cli --version

# Prod changelog feed — the stable client uses only its versioned path
curl -s https://prod.download.cli.kiro.dev/stable/<version>/feed.json | jq -r '.entries[].version'
# expect <version> listed; /changelog on that stable install renders it
```

## Revert a Bad Release

Two independent revert surfaces:

- **External release** — the public CDN path: install script + all auto-updaters (`q_cli`, desktop, Windows). Reverted by `scripts/revert-external-release.sh` (Step 3 below).
- **Internal toolbox** — `toolbox install kiro-cli` on Amazon-internal machines. Reverted by `scripts/revert-internal-release.sh` (Step 1 below). Distinct account, distinct tooling.

Do both if the bad version reached both surfaces. Skip Step 1 if the bug is external-only or toolbox never got the bad version.

Based on the proven procedure from ticket [P464326498](https://t.corp.amazon.com/P464326498/communication) (v2.11.0 rollback). See `artifact-endpoints.md` for the file/CDN mapping.

### Overview

| Step | Action | Blast radius |
|------|--------|--------------|
| 1 | Recall from toolbox (if internal users affected) | Internal toolbox — future installs + auto-updates |
| 2 | Re-run the release Lambda pointed at the previous version's artifacts | Overwrites `manifest.json`; keeps the bad version's `index.json` entry present |
| 3 | Run `revert-external-release.sh` in dry-run mode → inspect diff → re-run with `--live` | Downloads, strips, uploads + verifies, invalidates both CDNs |
| 4 | Verify from an unprivileged shell that both CDNs report the previous version | Confirmation only — no changes |
| 5 | Communicate the revert (Slack, ticket updates, follow-up regression ticket) | Coordination only |
| 6 | Cut a fix-forward patch release | Standard release flow — separate from this SOP |

Step 3 collapses download / strip / upload / verify / invalidate into one command. It defaults to a dry-run that stops before uploading so the operator can eyeball the change; `--live` runs the full pipeline including the CloudFront invalidations.

---

### Revert Step 1 — Recall the toolbox version (if internal users are affected)

If the bad version was published to the toolbox stable channel and internal users may have installed it, run the recall first. Skip this step if the bug only affects external users or if toolbox was never updated.

The recall marks the toolbox version as recalled so `toolbox update` runs pick up the recommended safe version instead. Existing installs stay on whatever they have until they run `toolbox update` — there is no push mechanism.

```bash
# Dry-run: previews the recall without applying
./.kiro/skills/release/scripts/revert-internal-release.sh \
    --version <bad_version> \
    --recommended <safe_version>

# Once the dry-run output looks right, execute:
./.kiro/skills/release/scripts/revert-internal-release.sh \
    --version <bad_version> \
    --recommended <safe_version> \
    --live
```

The script:
1. Auto-installs `toolbox-vendor-ops` if it isn't on PATH (via `toolbox install toolbox-ops`).
2. Refreshes Isengard creds for the toolbox account (`211125606403`, role `Admin`, profile `kiro-toolbox`).
3. Runs `toolbox-vendor-ops recall` against `s3://buildertoolbox-kiro-cli-us-west-2`. Uses `--dryrun` by default; `--live` executes.

Verify a fresh install after `--live`:
```bash
toolbox install kiro-cli --channel stable --force
kiro-cli --version   # must NOT report <bad_version>
```

Full doc on toolbox recall: https://docs.hub.amazon.dev/builder-toolbox/user-guide/vending-registry-management/

### Revert Step 2 — Re-run the release Lambda for the previous version

**What this fixes:** re-points `stable/latest/manifest.json` back to the previous version — the file `install.sh`/`install.ps1` reads. Future fresh installs (`curl cli.kiro.dev/install | bash`) now get the previous version. The Lambda invalidates both CDNs for `/stable/latest/*` and `/stable/index.json` as part of this run.

**What this does NOT fix:** the bad version's entry stays in `stable/index.json`. The Lambda's `check_version_exists` only removes the entry for the version currently being released (in this case, the *previous* version, with `overwrite_existing_entry=true`) — it has no code path that touches any other entry, so the bad version's entry is untouched. Auto-updaters read `index.json` and would still see the bad version as the newest. Revert Step 3 handles that.

```bash
# Get the previous version's autocomplete commit SHA
AUTOCOMPLETE_SHA=$(gh api repos/kiro-team/kiro-cli-autocomplete/branches/release/<previous_version> --jq '.commit.sha')

gh workflow run release-kiro-cli.yml \
    --repo kiro-team/kiro-cli-autocomplete \
    --ref release/<previous_version> \
    -f commit=$AUTOCOMPLETE_SHA \
    -f version=<previous_version> \
    -f channel=stable \
    -f branch_name=release/<previous_version> \
    -f environment=prod-release \
    -f release_to_cloudfront=true \
    -f release_to_toolbox=false \
    -f overwrite_existing_entry=true \
    -f enable_windows=true
```

Approve the `prod-release` gate in the GitHub UI. `release_to_toolbox=false` because Revert Step 1 already handled toolbox (if needed); `overwrite_existing_entry=true` because the previous version's entry already exists in `index.json`.

### Revert Step 3 — Strip the bad version, upload, verify, invalidate

`scripts/revert-external-release.sh` does everything for the **external surface**: downloads `index.json` from prod S3, removes the bad version, uploads the modified file back with matching content-type and cache-control, verifies the S3 object's SHA256 matches your local file, then invalidates both CloudFront distributions.

The strip upload happens via `aws s3 cp` directly, **bypassing the Lambda entirely** — so no Lambda-driven invalidation fires for this write. The script's own `aws cloudfront create-invalidation` calls are therefore the **only** invalidation that reaches clients for the stripped `index.json`. They are not redundant belt-and-suspenders on top of Revert Step 2's Lambda invalidations; they are the primary mechanism.

(This step does not touch toolbox — that's Revert Step 1's `toolbox-vendor-ops recall`.)

Defaults to **dry-run**. First run downloads and strips only, then stops so you can inspect the diff. Re-run with `--live` to execute upload + invalidate.

```bash
# 1. Load prod creds
eval "$(ada cred print --account 158872659206 --role McmDeploy --format env)"

# 2. Dry-run: download + strip, then stop
./.kiro/skills/release/scripts/revert-external-release.sh --version <bad_version>
# Produces:
#   results/index.origin.json  (untouched download)
#   results/index.json         (bad version removed)

# 3. Inspect the diff
diff <(jq -r '.versions[].version' results/index.origin.json) \
     <(jq -r '.versions[].version' results/index.json)
# expected: exactly one "< <bad_version>" line

# 4. If it looks right, execute the full revert
./.kiro/skills/release/scripts/revert-external-release.sh --version <bad_version> --live
```

The `--live` run:
1. Repeats download + strip (idempotent — same S3 file each time)
2. Re-downloads the remote and aborts if `index.json` changed since the initial download (guards against a concurrent write — e.g. the Revert Step 2 Lambda re-run — silently clobbering an intervening change)
3. Uploads `results/index.json` back to `s3://kiro-cli-public-download-prod-us-east-1-158872659206/stable/index.json` with `content-type: application/json` and `cache-control: public, max-age=300` (the intended registry-object metadata; the SHA256 check verifies body bytes only, not this metadata)
4. Downloads the freshly-uploaded S3 object and verifies SHA256 matches the local file
5. Invalidates both distributions with the correct per-CDN paths:
   - `E3I6IG70J7OAJ` (modern): `/stable/index.json` and `/stable/latest/*`
   - `E1PREM1JKPVIXA` (legacy): `/index.json` and `/latest/*` (no channel prefix)

Run Revert Step 2 (Lambda re-run) to completion **before** starting this step — both rewrite `index.json`, and the re-check in the `--live` run will refuse to upload if Step 2 is still in flight.

If SHA256 verification fails after upload, the script exits non-zero **before** invalidating CloudFront — safe fail.

The strip is byte-identical to the manual edit from the P464326498 rollback (same SHA256 as the ticket's `index(5).json`).

### Revert Step 4 — Verify

```bash
# Modern CDN — manifest.json (the file install.sh reads)
curl -s https://prod.download.cli.kiro.dev/stable/latest/manifest.json | jq -r .version
# expect <previous_version>

# Legacy CDN — index.json (auto-updater path). Bad version should NOT be listed.
curl -s https://desktop-release.q.us-east-1.amazonaws.com/index.json | jq -r '[.versions[].version] | index("<bad_version>")'
# expect: null

# End-to-end: fresh install from the install script
curl -fsSL https://cli.kiro.dev/install | bash
kiro-cli --version   # expect <previous_version>
```

If all three checks pass, the revert is live across every update path except toolbox (which Revert Step 1 handles).

### Revert Step 5 — Communicate

- Post in `#kiro-cli-internal-software-builders`: which version was reverted, which version customers are now on, workaround for toolbox users still on the bad version (`toolbox update`).
- File a Sev-3+ ticket for the underlying regression, linked from the tracking ticket for the reverted release.
- Update the tracking ticket for the bad release with the revert commands run and the follow-up ticket link.

### Revert Step 6 — Fix forward

The revert buys time; it doesn't close the incident. Cut a fix-forward patch release against the previous stable branch following the standard release flow (Release Steps 1–6 above).

## Troubleshooting

See [reference.md](reference.md) for supporting reference material — workflow reference table, known limitations, environment IDs (accounts), artifact paths, and branch-protection rules.

See [artifact-endpoints.md](artifact-endpoints.md) for the release-artifact file layout, CDN endpoints, and which install/update flow reads which file. Consult this if you need to understand *why* the revert steps do what they do.
