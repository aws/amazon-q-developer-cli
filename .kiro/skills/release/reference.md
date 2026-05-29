# Release Skill — Reference

## Workflow Reference

| Step | Workflow | Repo | Ref |
|------|----------|------|-----|
| Cut | `cut-release.yml` | chat | main |
| Cut | `create-release-branch.yml` | autocomplete | main |
| Build | `build-and-release.yml` | chat | release/X.Y.Z |
| Beta | `release-non-prod.yml` | autocomplete | release/X.Y.Z (auto-dispatched) |
| Prod | `promote-to-prod.yml` | autocomplete | release/X.Y.Z |

## Known Limitations

### PAT permissions
`AUTOCOMPLETE_TRIGGER_TOKEN` cannot create branches in autocomplete via API. Workaround: run `create-release-branch.yml` separately.
Ticket: https://t.corp.amazon.com/1230ee22-fa7d-4e97-9852-73d5818736e1

### Lambda timeout
Toolbox Lambda invocation can drop connection after ~13 min even though Lambda succeeds. The release still succeeds — re-run if workflow reports failure.
Ticket: https://t.corp.amazon.com/9bdbf9b2-e0f2-4a8b-a205-15d19794654e

### Tag force-update
`SyncVersionFromChat` in autocomplete force-updates the `v*` tag on every build. If you cancel and re-run, the tag moves to the new HEAD.

## Environment Details

| Environment | Account | Purpose |
|-------------|---------|---------|
| gamma-release | 230592382359 | Non-prod toolbox releases (nightly, beta) |
| prod-release | 158872659206 | Prod toolbox + CloudFront (requires approval) |

## Build Artifact Paths

Artifacts are stored in S3 at:
- Chat: `chat/<branch>/<commit>/<target>/`
- Autocomplete: `autocomplete/<branch>/<commit>/<target>/`

Both gamma and prod S3 buckets receive artifacts for `main`, `prod`, `nightly`, and `release/*` branches.

## Nightly (Automated)

Runs daily at 06:21 UTC via cron on `main`. No manual steps.

## Hotfix Process

1. Cherry-pick fix to `release/X.Y.Z` (via PR)
2. Build stable: `gh workflow run build-and-release.yml --ref release/X.Y.Z -f increment=stable`
3. Promote to prod (Step 7)

## Rollback

Re-promote the previous stable version:
```bash
gh workflow run promote-to-prod.yml \
  --repo kiro-team/kiro-cli-autocomplete \
  --ref main \
  -f version=<previous_version> \
  -f commit=<previous_autocomplete_sha> \
  -f chat_commit=<previous_chat_sha>
```

## Checking for Missing Reverts

Before releasing, check if any reverts on `main` need to be applied:
```bash
git log --oneline origin/release/X.Y.Z..origin/main | grep -i "revert"
```

## Branch Protection

`release/*` branches have protection rules:
- Require PR before merging (no direct pushes)
- No force pushes
- No deletions
