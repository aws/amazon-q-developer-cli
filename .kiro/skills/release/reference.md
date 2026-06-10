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

A **Hotfix** is a release that skips the RC verification cycle and goes straight to a stable build on top of an already-shipped stable version. It's the abbreviated form of the Standard release flow.

### When to use Hotfix vs Standard

Use **Hotfix** when *all* of the following are true:

- The change is a small, isolated, well-understood fix (typically a single commit or a few tightly related commits)
- The change is build/CI/config-only, or a clearly scoped runtime fix with a well-defined surface area
- There is no open customer-facing risk that needs broader pre-prod soak time
- Either: the fix is customer-blocking, or skipping the ~60–90 min RC cycle is otherwise valuable

Use the **Standard** flow (with RC) when *any* of the following are true:

- The release bundles multiple fixes, even if each is small individually
- The change touches behaviorally complex areas (chat loop, tool execution, MCP, auth, telemetry, conversation state)
- The change has uncertain user impact and would benefit from beta soak as `-rc`
- It's a minor or major release (`X.Y.0`)

When in doubt, default to Standard.

### Hotfix steps

Follows the main SKILL.md flow, but skips Steps 3 and 4 (RC build + RC verify):

1. **Step 1** — Cut `release/X.Y.Z` from the previous stable tag (`v(X.Y.Z-1)`) — both repos
2. **Step 2** — Cherry-pick the fix(es) onto `release/X.Y.Z` via PR; update `feed.json`
3. **Step 5** — Build stable:
   ```bash
   gh workflow run build-and-release.yml --ref release/X.Y.Z -f increment=stable
   ```
4. **Step 6** — Verify the stable build on the beta toolbox channel:
   ```bash
   toolbox install kiro-cli --channel=beta --force
   kiro-cli diagnostics      # version: X.Y.Z (no -rc)
   kiro-cli-chat diagnostics
   ```
   Capture both `hash` values for Step 7.
5. **Step 7** — Promote to prod (toolbox + CloudFront with the two manual approval gates)
6. **Step 8** — Verify production via toolbox and `cli.kiro.dev/install`

> **Why beta still gets used:** Even on the hotfix path, the stable build auto-releases to the beta toolbox channel before prod promotion. Step 6 is your one verification gate — don't skip it.

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
