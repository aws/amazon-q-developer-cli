# Release Artifacts: `index.json` vs `manifest.json`

Supplement to `SKILL.md`. Explains the two release-artifact JSON files, the two CDN endpoints each is exposed through, and which file+endpoint each install/update flow reads. Written from the code, with source refs.

## The two files

Both are written by `KiroCliDeployLambda`'s `release_handler.release_to_public` (invoked from `ac/.github/workflows/release-cloudfront.yml`) on every release. They live in the same S3 origin bucket, but have different shapes and different consumers.

### `index.json` — cumulative version registry

**Written at:** `<channel>/index.json` (e.g. `stable/index.json`)
**How:** Lambda downloads the existing file, appends the new version's entry in-memory, re-uploads. Cumulative — every version ever released to the channel is in this file.

**Shape:**
```json
{
  "versions": [
    {
      "version": "2.11.0",
      "packages": [{ "os": "macos", "architecture": "universal", "download": "...", "sha256": "...", ... }],
      "rollout": 100,
      "updateConditions": [{ "allowedAutoUpdateProductNames": [...] }]
    },
    ...
  ]
}
```

**Reader schema:** `ac/crates/fig_install/src/index.rs` — `pub struct Index { versions: Vec<RemoteVersion> }`.

**Why the readers use it:** it has the fields the update flow needs — `rollout` (staged rollouts), `updateConditions` (allow-lists), and the full per-arch package matrix (for AppImage vs deb vs rpm etc. selection).

### `manifest.json` — current "latest" snapshot

**Written at:** `<channel>/latest/manifest.json` (e.g. `stable/latest/manifest.json`)
**How:** Lambda writes it fresh each release — overwritten in full, no history. Represents only the current-latest version.

**Shape:**
```json
{
  "version": "2.11.0",
  "packages": [{ "os": "macos", "architecture": "universal", "download": "...", "sha256": "...", ... }]
}
```

**Reader schema:** `chat-cli/src/cli/update/manifest.rs` — `pub struct VersionManifest { version: String, packages: Vec<ArtifactEntry> }`.

**Why the readers use it:** minimal — just "what's the latest, where do I download it, what's the sha256." Fresh installs and simple update paths don't need version history or rollout gates.

## The two ways each file is exposed

Each file is reachable via **two distinct CloudFront distributions** with **different URL schemes** — one modern, one legacy. Both point at the same underlying S3 bucket.

### New (modern) endpoint — `prod.download.cli.kiro.dev`

- **Distribution:** `E3I6IG70J7OAJ` (CDK: `DownloadDistribution`)
- **Domain config:** `KiroCliDeployCDK/lib/config/config.ts` — `binaryDownloadDomain: 'prod.download.cli.kiro.dev'`
- **URL scheme:** channel prefix required — `/<channel>/...`
- **`index.json`** → `https://prod.download.cli.kiro.dev/stable/index.json`
- **`manifest.json`** → `https://prod.download.cli.kiro.dev/stable/latest/manifest.json`
- **Invalidated by:** Lambda directly, on every release, at the version, latest, and index paths (`KiroCliDeployLambda` — `release_handler.release_to_public`).

### Old (legacy) endpoint — `desktop-release.q.us-east-1.amazonaws.com`

- **Distribution:** `E1PREM1JKPVIXA` (CDK: `figIoDesktopDownloadCloudfrontDistributionArn` in prod)
- **Domain config:** `KiroCliDeployCDK/lib/config/config.ts` — the legacy fig-io/q-desktop CDN
- **URL scheme:** no channel prefix; root-level paths
- **`index.json`** → `https://desktop-release.q.us-east-1.amazonaws.com/index.json`
- **`manifest.json`** → `https://desktop-release.q.us-east-1.amazonaws.com/latest/manifest.json`
- **Invalidated by:** Lambda, on every prod release, at the version, latest, and index paths (`release_handler.release_to_public`). Note: **no channel prefix** on this distribution — the same S3 objects are exposed at root-level paths.
- **Where the default URL lives in code:** `ac/crates/fig_install/src/index.rs` — `DEFAULT_RELEASE_URL: &str = "https://desktop-release.q.us-east-1.amazonaws.com"`.

Both distributions read from the same S3 origin (`publicDownloadBucket`, per `KiroCliDeployCDK/lib/stacks/release-infra-stack.ts`), so the file **content** is the same regardless of which CDN a client hits. The choice of CDN is a URL-scheme / historical-compatibility issue, not a data-freshness one — but each has its own cache and thus its own invalidation.

## Scenario matrix — which file + endpoint each flow uses

| # | Scenario | Binary | Source file | Reads | Endpoint | Code reference |
|---|----------|--------|-------------|-------|----------|----------------|
| 1 | Install script (macOS/Linux) | `install.sh` | `ac/scripts/install.sh` | `manifest.json` | **New** — `prod.download.cli.kiro.dev/stable/latest/manifest.json` | `install.sh` (`MANIFEST_URL="${channel_base_url}/latest/manifest.json"`) |
| 1w | Install script (Windows) | `install.ps1` | `ac/scripts/install.ps1` | `manifest.json` | **New** — `prod.download.cli.kiro.dev/stable/latest/manifest.json` | `install.ps1` (`$ManifestUrl = "$BaseUrl/stable/latest/manifest.json"`) |
| 2a | Manual update — `chat_cli` (Kiro CLI, minimal install) | `kiro-cli` (chat_cli) | `chat-cli/src/cli/update/mod.rs` | `manifest.json` | **Old** — `desktop-release.q.us-east-1.amazonaws.com/latest/manifest.json` | `chat-cli/src/util/consts.rs` — `DEFAULT_UPDATE_MANIFEST_URL` still points at legacy URL (has a TODO to migrate) |
| 2b | Manual update — `q_cli` (with desktop) | `q` (q_cli) | `ac/crates/q_cli/src/cli/update.rs` → `fig_install::check_for_updates` | `index.json` | **Old** — `desktop-release.q.us-east-1.amazonaws.com/index.json` | `ac/crates/fig_install/src/index.rs` — `url.set_path("index.json")` on `RELEASE_URL` |
| 3a | Auto-update — macOS/Linux (desktop only) | `fig_desktop` background loop | `ac/crates/fig_desktop/src/update.rs` → `fig_install::update` | `index.json` | **Old** — `desktop-release.q.us-east-1.amazonaws.com/index.json` | Same path as 2b — `fig_install::check_for_updates` |
| 3b | Auto-update — Windows (no desktop required) | `q_cli` self-check | `ac/crates/fig_install/src/windows.rs` | `index.json` (for the check), then MSI `download` URL from that | **Old** — `desktop-release.q.us-east-1.amazonaws.com/index.json` | Same code path as 2b/3a for the check; MSI URL is `package.download` from index |

### Notes on the matrix

- **Kiro Desktop does not have an auto-update path in the code I traced.** Only `q_cli` (the legacy autocomplete-repo binary) has auto-update, and only when the fig_desktop process is running (macOS/Linux) or via self-check (Windows). Kiro CLI minimal-install users get updates only when they manually run `kiro-cli update` (scenario 2a).
- **Windows is the only OS where "no desktop, still auto-updates" holds true** — because the Windows update path (`fig_install/src/windows.rs`) is self-contained: it curls the MSI and runs it with `/upgrade /quiet /norestart`, no daemon needed. macOS/Linux `q_cli` needs `fig_desktop` running for its update loop.
- **Toolbox installs** (internal Amazon `toolbox install kiro-cli`) are a separate flow entirely. Not shown above — they use the toolbox's own `index.json` in the toolbox S3 bucket (schema `{Channels: [{Name: "stable"}]}`, written by `KiroCliDeployLambda` — `toolbox_accessor.ensure_channel_in_index`).
- **Every scenario in the table currently hits the same S3 bucket via one of two CDNs.** The New endpoint is used only by install (1) and its Windows counterpart (1w). Every update path — CLI or desktop, manual or auto — still routes through the legacy CDN because both `chat_cli`'s `DEFAULT_UPDATE_MANIFEST_URL` and `q_cli`'s `DEFAULT_RELEASE_URL` are hardcoded to `desktop-release.q.us-east-1.amazonaws.com`.

## Implications for reverts

Because updates in flight can hit **either** distribution, a revert that only invalidates one CDN is incomplete — some clients will keep pulling stale metadata from the other. This is why the 2.11.0 revert (see `SKILL.md` "Revert a Bad Release") explicitly invalidates both `E3I6IG70J7OAJ` and `E1PREM1JKPVIXA`.

The two writes in a revert have different invalidation coverage, and this matters:

- **Revert Step 2** re-runs `release-kiro-cli.yml` for the previous version. Every write in that run goes through the Lambda, and the Lambda's `invalidate_cloudfront_cache` fires for both CDNs at the end. But the Lambda's `check_version_exists` only removes the entry for the version being released — it never touches any other version. So Step 2's invalidation is **coverage without correction** for `index.json`: the CDN cache is cleared, but the origin still contains the bad version's entry.
- **Revert Step 3** (`scripts/revert-external-release.sh`) strips the bad entry and writes directly with `aws s3 cp`, bypassing the Lambda. That means the Lambda's invalidation loop is not triggered — Step 3's own `aws cloudfront create-invalidation` calls on both CDNs are the **only** cache purge for the stripped `index.json`. Without them, clients would keep serving the pre-strip file from CDN caches indefinitely.

Together the two steps are complementary: Step 2 fixes `manifest.json` (install-script path) and its cache; Step 3 fixes `index.json` (auto-updater path) and its cache.

## Follow-ups worth tracking (not blocking)

- **`chat-cli/src/util/consts.rs`** — `DEFAULT_UPDATE_MANIFEST_URL` still points at legacy CDN with a `TODO: Update this to the production manifest URL once the update infrastructure is deployed.` Migrating this to `prod.download.cli.kiro.dev/stable/latest/manifest.json` would reduce Kiro CLI's dependency on the legacy CDN.
- **`ac/crates/fig_install/src/index.rs`** — `DEFAULT_RELEASE_URL` similarly hardcoded to legacy CDN. Same class of follow-up for `q_cli`.
- **The FigIoDesktop distribution stays load-bearing** until both defaults migrate and all installed binaries auto-update off them. Any plan to sunset the legacy CDN needs to account for the long tail of installed clients still pointing at it.
