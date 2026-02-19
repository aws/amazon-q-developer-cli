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

3. Create PR and merge after approval.

## Step 4: Sync Changes to Prod Branch

**For standard release:**

1. In the `kiro-team/kiro-cli` repo:
   ```bash
   git checkout main && git pull origin main
   git checkout -b sync-prod-vX.X.X
   git push origin sync-prod-vX.X.X
   ```
   Create a PR from `sync-prod-vX.X.X` → `prod` and merge after approval.

2. In the `kiro-team/kiro-cli-autocomplete` repo:
   ```bash
   git checkout main && git pull origin main
   git checkout -b sync-prod-vX.X.X
   git push origin sync-prod-vX.X.X
   ```
   Create a PR from `sync-prod-vX.X.X` → `prod` and merge after approval.

**For hotfix release:**

1. In the `kiro-team/kiro-cli` repo:
   ```bash
   git checkout prod && git pull origin prod
   git checkout -b hotfix-vX.X.X
   git cherry-pick <commit-sha> # cherry pick all commits as required
   git push origin hotfix-vX.X.X
   ```
   Create a PR from `hotfix-vX.X.X` → `prod` and merge after approval.

2. In the `kiro-team/kiro-cli-autocomplete` repo:
   ```bash
   git checkout prod && git pull origin prod
   git checkout -b hotfix-vX.X.X
   git cherry-pick <commit-sha> # cherry pick all commits as required
   git push origin hotfix-vX.X.X
   ```
   Create a PR from `hotfix-vX.X.X` → `prod` and merge after approval.

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

## Step 6: Deploy to Toolbox Beta Channel

1. Get the commit SHA from the autocomplete prod build:
   ```bash
   gh run list --repo kiro-team/kiro-cli-autocomplete --branch prod --limit 1 --json headSha --jq '.[0].headSha'
   ```

2. Trigger the release workflow:
   ```bash
   gh workflow run release-kiro-cli.yml \
     --repo kiro-team/kiro-cli-autocomplete \
     -f commit=<COMMIT_SHA> \
     -f version=<VERSION> \
     -f channel=beta \
     -f branch_name=prod \
     -f environment=gamma-release \
     -f release_to_cloudfront=false \
     -f release_to_toolbox=true
   ```

## Step 7: Verify Toolbox Beta Installation

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

## Step 8: Run Release Prod Workflow

1. Trigger the production release:
   ```bash
   gh workflow run release-kiro-cli-prod.yml \
     --repo kiro-team/kiro-cli-autocomplete \
     -f commit=<COMMIT_SHA> \
     -f version=<VERSION> \
     -f channel=stable \
     -f release_to_cloudfront=true \
     -f release_to_toolbox=true
   ```

2. Approve the release in GitHub UI (`prod-release` environment requires manual approval).

## Step 9: Verify Release Workflow Completes

1. Monitor the release workflow:
   ```bash
   gh run list --repo kiro-team/kiro-cli-autocomplete --workflow release-kiro-cli-prod.yml --limit 1
   ```

2. Wait for status to show `completed` with `success` conclusion.

## Step 10: Verify Stable Release

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

## Release Tracker

Create a release tracker at `docs/oncall/releases/vX.Y.Z.md` using the [template](releases/TEMPLATE.md) to document progress through each step.

For detailed build/release infrastructure, see [Build & Release Process](build_release_process.md).
