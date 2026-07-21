# Release Skill — Reference

Supporting reference for `SKILL.md`. Consult when troubleshooting or when you need infrastructure details that would clutter the main SOP.

For the release flow and the full revert-a-bad-release SOP, see `SKILL.md`.
For the release-artifact file layout (`index.json` vs `manifest.json`, CDN endpoints, per-scenario file mapping), see `artifact-endpoints.md`.

## Workflow Reference

| Step | Workflow | Repo | Ref |
|------|----------|------|-----|
| Cut | `cut-release.yml` | chat | main |
| Cut | `create-release-branch.yml` | autocomplete | main |
| Build | `build-and-release.yml` | chat | release/X.Y.Z |
| Beta | `release-non-prod.yml` | autocomplete | release/X.Y.Z (auto-dispatched) |
| Prod | `promote-to-prod.yml` | autocomplete | release/X.Y.Z |
| Revert (external CDN) | `release-kiro-cli.yml` | autocomplete | release/<previous_version> |

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
| toolbox-vendor-ops | 211125606403 | Toolbox registry / recall operations (used by `revert-internal-release.sh`) |

## Build Artifact Paths

Artifacts are stored in S3 at:
- Chat: `chat/<branch>/<commit>/<target>/`
- Autocomplete: `autocomplete/<branch>/<commit>/<target>/`

Both gamma and prod S3 buckets receive artifacts for `main`, `prod`, `nightly`, and `release/*` branches.

## Nightly (Automated)

Runs daily at 06:21 UTC via cron on `main`. No manual steps.

## Branch Protection

`release/*` branches have protection rules:
- Require PR before merging (no direct pushes)
- No force pushes
- No deletions

## Related documents

- **`SKILL.md`** — the release SOP and the revert-a-bad-release SOP.
- **`artifact-endpoints.md`** — file layout, CDN endpoints, per-scenario reader table.
- **`scripts/revert-external-release.sh`** — CDN revert (download → strip → upload → verify → invalidate).
- **`scripts/revert-internal-release.sh`** — toolbox recall (auto-installs `toolbox-vendor-ops`, refreshes creds, runs recall).
