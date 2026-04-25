---
name: kiro-cli-release-sop
description: Kiro CLI release standard operating procedure. Use when performing version bumps, bug bashes, deploying to toolbox, or releasing to production.
---

# Kiro CLI Release SOP

## Prerequisites

- `gh` CLI installed and authenticated (`gh auth status`)
- Push access to `kiro-team/kiro-cli` and `kiro-team/kiro-cli-autocomplete`
- For bug bash with AI assistance: QuipEditor tool available (install `builder-mcp`)

## Step 0: Determine Release Type

1. Decide the release type:
   - **Standard release**: Sync `main` → `prod` (includes all changes since last release)
   - **Hotfix release**: Cherry-pick specific commits from `main` → `prod`

2. Review changes for documentation updates:
   - Check `.changes/unreleased/` for user-facing changes
   - Coordinate with the team and docs POC @jayrava in the #kiro-cli-team Slack channel

## Step 1: Bug Bash

The current approach is to bug bash the nightly build.

1. Run the `@bugbash` prompt and provide:
   - Version number
   - Commit SHA to test

2. Share the generated Quip doc with the team.

3. Iterate on any blocking issues until the team approves the release.

## Step 2: Version Bump CLI Repo on Main

1. Sync with latest main:
   ```bash
   git checkout main && git pull origin main
   ```

2. Run the `@release` prompt and provide the version when asked. This will:
   - Update version in `Cargo.toml`
   - Update `Cargo.lock`
   - Generate changelog from `.changes/unreleased/`
   - Create release branch and PR

3. Merge the PR after approval.

## Step 3: Version Bump Autocomplete Repo

1. In the `kiro-team/kiro-cli-autocomplete` repo, sync with latest main:
   ```bash
   git checkout main && git pull origin main
   ```

2. Update version in `Cargo.toml` and update lockfile:
   ```bash
   # Update version in Cargo.toml to match CLI repo version
   cargo check
   ```

3. Sync `feed.json` from the kiro-cli repo to the repo root.

4. Create PR and merge after approval.

## Step 4: Sync Changes to Prod Branch

**For standard release:**

1. In the `kiro-team/kiro-cli` repo:
   ```bash
   git checkout main && git pull origin main
   git checkout -b sync-prod-vX.X.X
   git push origin sync-prod-vX.X.X
   gh pr create --repo kiro-team/kiro-cli --base prod --head sync-prod-vX.X.X --title "Sync main to prod for vX.X.X" --body "Standard release sync for vX.X.X"
   ```
   Merge after approval.

2. In the `kiro-team/kiro-cli-autocomplete` repo:
   ```bash
   git checkout main && git pull origin main
   git checkout -b sync-prod-vX.X.X
   git push origin sync-prod-vX.X.X
   gh pr create --repo kiro-team/kiro-cli-autocomplete --base prod --head sync-prod-vX.X.X --title "Sync main to prod for vX.X.X" --body "Standard release sync for vX.X.X"
   ```
   Merge after approval.

**For hotfix release:**

1. In the `kiro-team/kiro-cli` repo:
   ```bash
   git checkout prod && git pull origin prod
   git checkout -b hotfix-vX.X.X
   git cherry-pick <commit-sha> # cherry pick all commits as required
   git push origin hotfix-vX.X.X
   gh pr create --repo kiro-team/kiro-cli --base prod --head hotfix-vX.X.X --title "Hotfix vX.X.X" --body "Hotfix release for vX.X.X"
   ```
   Merge after approval.

2. In the `kiro-team/kiro-cli-autocomplete` repo:
   ```bash
   git checkout prod && git pull origin prod
   git checkout -b hotfix-vX.X.X
   git cherry-pick <commit-sha> # cherry pick all commits as required
   git push origin hotfix-vX.X.X
   gh pr create --repo kiro-team/kiro-cli-autocomplete --base prod --head hotfix-vX.X.X --title "Hotfix vX.X.X" --body "Hotfix release for vX.X.X"
   ```
   Merge after approval.

> **Note: Out-of-Sync Branches**
>
> If `main` and `prod` are out of sync and a PR can't be made without merge conflicts, the current process is to force push `main` to `prod`:
> ```bash
> git checkout main && git pull origin main
> git push --force origin main:prod
> ```

## Step 5: Wait for Builds to Succeed

1. Monitor CLI repo builds:
   ```bash
   gh run list --repo kiro-team/kiro-cli --branch prod --limit 5
   ```

2. Monitor autocomplete repo builds:
   ```bash
   gh run list --repo kiro-team/kiro-cli-autocomplete --branch prod --limit 5
   ```

3. Wait for all builds to show `completed` with `success` conclusion.

## Step 6: Create Release Tracking Ticket

Create a tracking ticket for the release:

- **Title**: `Kiro CLI <VERSION>`
- **CTI**: `Kiro / CLI / Intake`
- **Severity**: Sev-5
- **Description**: `Tracking ticket for the release of Kiro CLI <VERSION>`

Document each deployment step (toolbox beta, prod, CloudFront) as comments on this ticket with the `gh workflow run` commands used and links to the GitHub Actions runs.

## Step 7: Deploy to Toolbox Beta Channel

1. Get the commit SHA from the autocomplete prod build:
   ```bash
   gh run list --repo kiro-team/kiro-cli-autocomplete --branch prod --limit 1 --json headSha --jq '.[0].headSha'
   ```

2. Trigger the release workflow:
   ```bash
   gh workflow run release-kiro-cli.yml \
     --repo kiro-team/kiro-cli-autocomplete \
     --ref prod \
     -f commit=<COMMIT_SHA> \
     -f version=<VERSION> \
     -f channel=beta \
     -f branch_name=prod \
     -f environment=gamma-release \
     -f release_to_cloudfront=false \
     -f release_to_toolbox=true
   ```

## Step 8: Verify Toolbox Beta Installation

1. Install from toolbox beta:
   ```bash
   toolbox install kiro-cli --channel beta --force
   ```

2. Verify versions match expected:
   ```bash
   kiro-cli diagnostic
   kiro-cli-chat diagnostic
   ```

3. Confirm both show:
   - Expected version number
   - Commit hash matching `origin/prod` from their respective repos

4. On Windows, verify:
   ```powershell
   toolbox install kiro-cli --channel beta --force
   kiro-cli --version
   kiro-cli diagnostic
   ```

## Step 9: Run Release Prod Workflow

1. Trigger the production release:
   ```bash
   gh workflow run release-kiro-cli-prod.yml \
     --repo kiro-team/kiro-cli-autocomplete \
     --ref prod \
     -f commit=<COMMIT_SHA> \
     -f version=<VERSION> \
     -f channel=stable \
     -f release_to_cloudfront=true \
     -f release_to_toolbox=true
   ```

2. Approve the release in GitHub UI (`prod-release` environment requires manual approval).

## Step 10: Verify Release Workflow Completes

1. Monitor the release workflow:
   ```bash
   gh run list --repo kiro-team/kiro-cli-autocomplete --workflow release-kiro-cli-prod.yml --limit 1
   ```

2. Wait for status to show `completed` with `success` conclusion.

## Step 11: Verify Stable Release

1. Install from toolbox stable:
   ```bash
   toolbox install kiro-cli --channel stable --force
   ```

2. Verify versions match expected:
   ```bash
   kiro-cli diagnostic
   kiro-cli-chat diagnostic
   ```

3. Confirm both show:
   - Expected version number
   - Commit hash matching `origin/prod` from their respective repos

4. On Windows, verify:
   ```powershell
   toolbox install kiro-cli --channel stable --force
   kiro-cli --version
   kiro-cli diagnostic
   ```

## Recalling a Toolbox Version

Use this when a released version has a critical issue and needs to be pulled from toolbox.

### Prerequisites

Install `toolbox-vendor-ops` if not already available:

```bash
toolbox registry add s3://buildertoolbox-registry-toolbox-ops-us-west-2/tools.json \
  && toolbox install toolbox-ops
```

### Account & Credentials

The toolbox bucket lives in account **`211125606403`** (separate from the Gamma/Prod deploy accounts). You need credentials for this account.

```bash
ada credentials update --account 211125606403 --provider isengard --role Admin --profile kiro-toolbox --once
```

### Recall Steps

1. **Dry run first** to verify what will change:
   ```bash
   toolbox-vendor-ops \
     --credentials-file ~/.aws/credentials \
     --profile kiro-toolbox \
     --repo s3://buildertoolbox-kiro-cli-us-west-2 \
     --dryrun \
     recall \
     --version <VERSION_TO_RECALL> \
     --channel stable \
     --recommended <SAFE_VERSION>
   ```

2. **Execute the recall** (remove `--dryrun`):
   ```bash
   toolbox-vendor-ops \
     --credentials-file ~/.aws/credentials \
     --profile kiro-toolbox \
     --repo s3://buildertoolbox-kiro-cli-us-west-2 \
     recall \
     --version <VERSION_TO_RECALL> \
     --channel stable \
     --recommended <SAFE_VERSION>
   ```

3. **Verify** the recalled version is no longer installable:
   ```bash
   toolbox install kiro-cli --channel stable --force
   kiro-cli diagnostic
   ```

### Key Flags

| Flag | Description |
|------|-------------|
| `--version` | Version to recall (required) |
| `--channel` | `stable`, `beta`, or `nightly` (omit to recall from all channels) |
| `--os` | `osx`, `ubuntu`, `alinux`, `windows` (omit to recall on all OSes) |
| `--recommended` | Version customers should auto-update to |
| `--dryrun` | Preview changes without applying |

### Undoing a Recall

```bash
toolbox-vendor-ops \
  --repo s3://buildertoolbox-kiro-cli-us-west-2 \
  unrecall \
  --version <VERSION> \
  --make-current
```

> **Note:** Customers on a recalled version stay on it until `toolbox update` runs — there is no way to push an update. The recall only affects new installs and auto-updates.

### References

- [Builder Toolbox: Vending & Registry Management](https://docs.hub.amazon.dev/builder-toolbox/user-guide/vending-registry-management/) — full docs for publishing, recalling, and managing tool registries
- [BuilderToolboxOpsTools package](https://code.amazon.com/packages/BuilderToolboxOpsTools) — source for `toolbox-vendor-ops` and `toolbox-registry` commands

## Release Tracker

Create a release tracker at `docs/oncall/releases/vX.Y.Z.md` using the [template](releases/TEMPLATE.md) to document progress through each step.

For detailed build/release infrastructure, see [Build & Release Process](build_release_process.md).
