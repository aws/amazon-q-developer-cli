---
name: publish-changelog
description: Publish or fix the hosted changelog feed that Kiro CLI fetches at runtime. Use for ad hoc feed updates (typo fixes, added entries, content corrections) outside the release flow, or to verify/re-run the automatic release-time publishes. Triggers on "publish changelog", "update feed", "changelog feed", "fix changelog content".
---

# Publish Changelog Feed

The CLI fetches its changelog at runtime from a hosted feed instead of relying only on the copy compiled into the binary. This skill covers how the feed gets published — automatically during releases and manually for ad hoc content fixes.

## How the feed works

- Source of truth: `crates/chat-cli/src/cli/feed.json` on the ref being published.
- Publisher: `.github/workflows/publish-changelog.yml` (chat repo). Validates against `feed-schema.json`, strips non-`release` and `hidden` entries, refuses to publish an empty feed, uploads to S3, invalidates CloudFront.
- Endpoints published for each stable release:
  - Gamma versioned: `https://download.gamma.cli.kiro.dev/stable/<version>/feed.json`
  - Gamma rolling: `https://download.gamma.cli.kiro.dev/stable/changelog/feed.json`
  - Prod versioned: `https://prod.download.cli.kiro.dev/stable/<version>/feed.json`
- Stable clients fetch only their own prod versioned feed. Nightly/rc/feature clients read the gamma rolling feed because a feed is not published for every prerelease artifact version.
- Clients fetch async on launch and blocking on `/changelog`, cache with ETag, cap entries at the running binary version, and fall back to their matching cache then the embedded copy. Publishes invalidate every path written in that environment; `max-age=300` bounds staleness at ~5 minutes if an invalidation is ever skipped.

## Automatic publishes (release flow — no action needed)

| When | What | Where |
|------|------|-------|
| Stable build (`build-and-release.yml`, non-rc) | Publishes the release branch's feed.json to the versioned and rolling paths in **gamma** | chat repo, `publish_changelog` job ("Publish Changelog to Gamma") |
| Promote to prod (`promote-to-prod.yml` in autocomplete repo) | Dispatches `publish-changelog.yml` on the release branch to the versioned path in **prod**, in lockstep with the CloudFront release | autocomplete repo, `publish-changelog` job |

If a release ran but the feed didn't update, check those jobs first before publishing manually.

## Ad hoc publish (content fix outside a release)

Use when feed content needs to change without shipping a binary — a typo in an entry, a missing change line, or a date correction.

1. Fix `crates/chat-cli/src/cli/feed.json` **on the release branch of the currently shipped stable version** via PR (branch protection requires PRs). The published feed must describe what users can actually install, so the release branch — not `main` — is the ref to publish from.

2. Dispatch the publisher against that branch:
   ```bash
   # Prod (stable users). The prod environment may require an approval in the GitHub UI.
   gh workflow run publish-changelog.yml --ref release/<version> -f environment=prod

   # Gamma (nightly/rc users) — usually only needed if gamma content is also wrong.
   gh workflow run publish-changelog.yml --ref release/<version> -f environment=gamma
   ```

3. Watch the run:
   ```bash
   gh run list --workflow publish-changelog.yml --limit 1
   gh run watch <run_id> --exit-status
   ```

4. Verify the public objects (no VPN needed):
   ```bash
   # Prod writes only the stable client's version-scoped object.
   curl -s https://prod.download.cli.kiro.dev/stable/<version>/feed.json | jq -r '.entries[].version'

   # Gamma writes both: stable-build verification plus the prerelease rolling feed.
   curl -s https://download.gamma.cli.kiro.dev/stable/<version>/feed.json | jq -r '.entries[].version'
   curl -s https://download.gamma.cli.kiro.dev/stable/changelog/feed.json | jq -r '.entries[].version'
   ```
   Then end-to-end on a stable install: run `/changelog` (blocking fresh fetch) and confirm the corrected content renders.

## Safety properties (why a bad publish is hard)

- Schema validation + required-field checks run before upload; a malformed feed never publishes.
- Non-`release` and `hidden` entries are stripped; unreleased content on the ref cannot leak.
- An empty result skips the publish entirely rather than blanking the hosted feed.
- Clients sanitize and version-cap what they render, and fall back to cache/embedded on any fetch or parse failure.
- The publisher writes the immutable versioned object in both environments and updates the rolling object only in gamma, where prerelease clients need it.
- Per-user escape hatch: `KIRO_NO_REMOTE_CHANGELOG=1` disables remote fetch entirely.

## Rollback

The workflow updates the version-scoped `stable/<version>/feed.json` used by that stable client. In gamma it also updates `stable/changelog/feed.json` for prerelease clients. To roll back content, re-dispatch the workflow from a ref whose feed.json has the desired content (e.g., the release branch commit before the bad edit). The ref you publish from is the rollback mechanism.
