---
name: ci-cd-documentation
description: Kiro CLI CI/CD workflow documentation. Use when debugging workflow failures, understanding triggers, managing cross-repo tokens, or troubleshooting nightly builds.
---

# Kiro CLI CI/CD Documentation

This document provides comprehensive documentation of the CI/CD workflows, dependencies, triggers, and secrets for the `kiro-cli` (chat) and `kiro-cli-autocomplete` repositories.

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Repository Structure](#repository-structure)
3. [Nightly Build System](#nightly-build-system)
4. [Release Systems](#release-systems)
5. [Workflow Reference](#workflow-reference)
6. [Secrets Reference](#secrets-reference)
7. [Cross-Repository Triggers](#cross-repository-triggers)
8. [Security Monitoring](#security-monitoring)
9. [Troubleshooting](#troubleshooting)

---

## Architecture Overview

The Kiro CLI consists of two main components built from separate repositories:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           KIRO CLI BUILD SYSTEM                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌─────────────────────┐         triggers          ┌─────────────────────┐  │
│  │    kiro-cli         │ ─────────────────────────▶│ kiro-cli-autocomplete│  │
│  │    (Chat Binary)    │   AUTOCOMPLETE_TRIGGER    │   (Desktop App)      │  │
│  │                     │         _TOKEN            │                      │  │
│  └─────────────────────┘                           └─────────────────────┘  │
│           │                                                  │               │
│           │ uploads to S3                                    │ downloads     │
│           ▼                                                  ▼ chat binary   │
│  ┌─────────────────────────────────────────────────────────────────────┐    │
│  │                         AWS S3 Build Bucket                          │    │
│  │   chat/{branch}/{commit}/{target}/kiro-cli-chat.zip                 │    │
│  │   autocomplete/{branch}/{commit}/{platform}/artifacts               │    │
│  └─────────────────────────────────────────────────────────────────────┘    │
│                                                                              │
│  ┌─────────────────────┐         triggers          ┌─────────────────────┐  │
│  │ kiro-cli-autocomplete│ ─────────────────────────▶│    kiro-cli         │  │
│  │ (Slack Notification) │   CHAT_TRIGGER_TOKEN     │ (nightly-release-   │  │
│  │                      │                          │  notification.yml)  │  │
│  └─────────────────────┘                           └─────────────────────┘  │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Repository Structure

### kiro-cli (Chat Repository)

The chat binary - the core CLI functionality.

| Workflow | Purpose | Trigger |
|----------|---------|---------|
| `build-and-release.yml` | **Current orchestrator** (Release SOP 2.0) - computes tag, builds chat for all platforms, dispatches autocomplete | Schedule (06:30 + 18:30 UTC), manual |
| `cut-release.yml` | Creates `release/X.Y.Z` branch from a base tag (Release SOP 1.0) | Manual |
| `build-darwin.yml` | Builds macOS universal binary | Called by build-and-release.yml |
| `build-linux.yml` | Builds Linux binaries (4 targets) | Called by build-and-release.yml |
| `build-windows.yml` | Builds Windows binary + MSI | Called by build-and-release.yml |
| `generate-embeddings.yml` | Generates autodocs embeddings | Called by build-and-release.yml |
| `smoke-tests.yml` | Non-blocking post-build validation (rust + kas engines) | Called by build-and-release.yml |
| `nightly-release-notification.yml` | Sends Slack notification for nightly releases | Called by autocomplete or manual |
| `notify-slack.yml` | Generic Slack notification helper | Called by other workflows |
| `rust.yml` | CI - Clippy, tests, fmt, deny | Push, PR |
| `release-notification.yaml` | Notifies on GitHub releases | Release published |
| `validate-commits.yml` | Validates changelog fragments | PR |
| `check-merge-conflicts.yml` | Checks for conflicts with main | PR |
| `typos.yml` | Spell checking | Push |
| `check-bun-version.yml` | Checks for new Bun releases, opens issue if outdated | Schedule (daily 9 UTC), manual |
| `osv-scan.yml` | OSV vulnerability scanning (bun.lock, bundled Bun binary) | PR, push to main, schedule (daily 9 UTC), manual |

> **Deprecated:** `build-kiro-cli.yml` was the previous orchestrator (daily 6:21 UTC schedule + push). Its schedule and push triggers are commented out; it remains for manual `workflow_dispatch` only during the transition. Do not rely on it to know whether a nightly is running - check `build-and-release.yml` instead.

### kiro-cli-autocomplete (Desktop Repository)

The desktop application that bundles the chat binary.

| Workflow | Purpose | Trigger |
|----------|---------|---------|
| `build-kiro-cli.yml` | Builds autocomplete for all platforms | workflow_call, workflow_dispatch, push |
| `build-darwin.yml` | Builds macOS app (signed & notarized) | Called by build-kiro-cli.yml |
| `build-linux.yml` | Builds Linux packages (5 variants) | Called by build-kiro-cli.yml |
| `release-non-prod.yml` | Non-prod (nightly/beta/feature) release pipeline (Release SOP 2.2). Renamed from `release-nightly.yml` | workflow_dispatch (from chat repo) |
| `create-release-branch.yml` | Creates `release/X.Y.Z` branch (mirror of chat's cut-release) | Manual |
| `promote-to-prod.yml` | Promotes a release branch from gamma to prod (Release SOP 3.0) | Manual |
| `release-kiro-cli-prod.yml` | Production release | Manual |
| `release-toolbox.yml` | Releases to internal toolbox | Called by release workflows |
| `release-cloudfront.yml` | Releases to public CloudFront | Called by release workflows |
| `rust.yml` | CI - Clippy, tests, fmt, deny | Push, PR |
| `typescript.yml` | CI - TypeScript tests and lint | Push, PR |
| `npm-publish.yml` | Publishes npm packages | Manual |
| `check-merge-conflicts.yml` | Checks for conflicts with main | PR |
| `typos.yml` | Spell checking | Push, PR |

---

## Nightly Build System

Nightlies are driven by **`build-and-release.yml`** ("Release SOP: 2.0 Build and Trigger Release") in the **chat repo** - a single tag-based orchestrator that replaced the old two-phase `build-kiro-cli.yml` + `release-nightly.yml` flow.

**Schedule:** twice daily via cron - `30 6 * * *` (11:30 PM PST) and `30 18 * * *` (11:30 AM PST). It can also be run manually via `workflow_dispatch` with an `increment` input (`nightly` / `rc` / `stable` / `feature`).

A scheduled run computes the next tag, pushes it, builds all platforms, and dispatches the autocomplete release - all coordinated from this one workflow.

```
PHASE 1: Tag + Build (kiro-cli repo, build-and-release.yml)
═══════════════════════════════════════════════════════════

  schedule (06:30 / 18:30 UTC) or workflow_dispatch
           │
           ▼
  tag (Create Tag)
   - "Check if main advanced since last nightly tag" → skips if no new commits
   - Computes next tag, e.g. vX.Y.Z-nightly.N, and pushes it
           │
           ▼
  prepare (Derive Release Metadata)
   - Derives version, channel, branch_name, build_timestamp, environment matrix
   - channel: nightly | stable | <feature-slug>
           │
           ├─────────────┬──────────────┬───────────────┐
           ▼             ▼              ▼               ▼
  generate_embeddings  build_darwin  build_linux   build_windows (+ build_windows_retry)
   (autodocs)          (universal)   (4 targets)   (binary + MSI)
           │             │              │               │
           └─────────────┴──────┬───────┴───────────────┘
                                ▼
              (all platform builds succeeded?)
                    │                        │
                    ▼                        ▼
            smoke_tests              dispatch_autocomplete
          (non-blocking,            (gamma env, uses
           rust + kas engines)       AUTOCOMPLETE_TRIGGER_TOKEN)
                                            │
                                            ▼

PHASE 2: Autocomplete Release (kiro-cli-autocomplete repo)
══════════════════════════════════════════════════════════

  release-non-prod.yml (dispatched with release_quality, chat_binary_commit,
  chat_version, chat_build_timestamp, trigger_source=chat-build)
   - channel→release_quality mapping: nightly→nightly, stable→beta, feature→<slug>
           │
           ▼
  Sync version from chat → build Darwin (sign+notarize) + Linux
   - Downloads the chat binary from S3
           │
           ▼
  release_toolbox → GitHub release (with build manifest) → notify_slack
   (Slack notification triggers nightly-release-notification.yml in chat repo
    via CHAT_TRIGGER_TOKEN)
```

> **Where do I look to know if a nightly/release is building right now?**
> The chat-repo orchestrator is the single source of truth:
> ```bash
> gh run list --repo kiro-team/kiro-cli \
>   --workflow="Release SOP: 2.0 Build and Trigger Release" --limit 5
> ```
> A `schedule` trigger on `main` is a nightly; a `workflow_dispatch` on `release/X.Y.Z` is a cut release. Ignore `build-kiro-cli.yml` runs - that workflow is deprecated.

### KAS Bundling

The KAS (Kiro Agent Server) bundle is embedded in builds via the `INCLUDE_KAS_BUNDLE` env var consumed by `scripts/build.py`. As of June 2026 KAS ships in **all** builds (nightly, rc, stable, feature) on all platforms - `INCLUDE_KAS_BUNDLE` is hardcoded to `"true"` in `build-darwin.yml`, `build-linux.yml`, and `build-windows.yml`, and the Darwin `notarize-node` job runs unconditionally (Node is required for the KAS bundle). Previously KAS was gated to nightly-only (`channel == 'nightly'`).

### Version Numbering

Versions come from git tags computed by `build-and-release.yml`:

- **Nightly**: `vX.Y.Z-nightly.N` - from `main` (e.g., `v2.6.2-nightly.1`)
- **RC**: `vX.Y.Z-rc.N` - from `release/X.Y.Z`
- **Stable**: `vX.Y.Z` - from `release/X.Y.Z` (e.g., `v2.6.0`)
- **Feature**: `vX.Y.Z-{slug}.N` - from `feature/{slug}`

Chat and autocomplete tag in lockstep at the same version.

### Branch Strategy

| Branch | Purpose | Tag computed |
|--------|---------|----------------|
| `main` | Development; source of nightlies | `-nightly.N` (gamma only) |
| `release/X.Y.Z` | Release stabilization | `-rc.N`, then `vX.Y.Z` (gamma + prod) |
| `feature/{slug}` | Feature development | `-{slug}.N` (gamma only) |

> The old standalone `nightly` and `prod` long-lived branches are no longer part of the flow; releases branch from tags via `cut-release.yml`.

---

## Release Systems

### Nightly Release (Automatic)

Computed and built twice daily (06:30 + 18:30 UTC) by `build-and-release.yml` on `main`.

```
Tag (-nightly.N) → Chat Build (all platforms, gamma) → dispatch_autocomplete
  → release-non-prod.yml (build + toolbox + GitHub release) → Slack Notification
```

### Cut a Release (Release SOP 1.0)

1. `cut-release.yml` (chat) + `create-release-branch.yml` (autocomplete) - create the
   `release/X.Y.Z` branch from a base tag.
   **Inputs:** `version` (e.g. `2.5.0`), `base_tag` (e.g. `v2.4.1-nightly.14`).
2. Run `build-and-release.yml` (Release SOP 2.0) on the `release/X.Y.Z` branch with
   `increment=rc` (or `stable`) to tag and build gamma + prod artifacts.

### Promote to Production (Release SOP 3.0)

`promote-to-prod.yml` (autocomplete) promotes a release branch's gamma artifacts to prod
(CloudFront + Toolbox). The legacy `release-kiro-cli-prod.yml` remains as the underlying
production release path.

---

## Workflow Reference

### kiro-cli Workflows

#### 1.0 Build and Trigger Release (build-and-release.yml) - "Release SOP: 2.0"

The current orchestrator (replaced `build-kiro-cli.yml`).

**Triggers:**
- `schedule`: `30 6 * * *` and `30 18 * * *` (nightly, twice daily)
- `workflow_dispatch`: Manual with `increment` (rc/stable/nightly/feature) and `dry_run`

**Jobs Flow:**
```
tag → prepare → generate_embeddings
                      │
        ┌─────────────┼──────────────┬───────────────┐
        ▼             ▼              ▼               ▼
  build_darwin   build_linux   build_windows   build_windows_retry
        │             │              │               │
        └─────────────┴──────┬───────┴───────────────┘
                             ▼
              smoke_tests (non-blocking) + dispatch_autocomplete
                             │
                             ▼
                          summary
```

`tag` self-skips a scheduled run if `main` has not advanced since the last nightly tag.
`dispatch_autocomplete` runs in the `gamma` environment and uses `AUTOCOMPLETE_TRIGGER_TOKEN`
to dispatch `release-non-prod.yml` in the autocomplete repo.

#### 1.1 Build Linux (build-linux.yml)

**Called by:** build-and-release.yml

**Build Matrix:**
| Target | Runner | Notes |
|--------|--------|-------|
| x86_64-unknown-linux-gnu | ubuntu-22.04 | Standard |
| x86_64-unknown-linux-musl | ubuntu-22.04 | Static linking |
| aarch64-unknown-linux-gnu | ubuntu-22.04 | Cross-compiled |
| aarch64-unknown-linux-musl | ubuntu-22.04 | Cross-compiled, static |

#### 1.2 Build Darwin (build-darwin.yml)

**Called by:** build-and-release.yml

**Output:** Universal binary (x86_64 + aarch64)

### kiro-cli-autocomplete Workflows

#### 1.0 Build Kiro CLI (build-kiro-cli.yml)

**Triggers:**
- `workflow_call`: From release-non-prod.yml
- `workflow_dispatch`: Manual
- `push`: To prod, main, feature/*

**Inputs:**
| Input | Type | Default | Description |
|-------|------|---------|-------------|
| `release_quality` | string | insider | stable/nightly/insider |
| `chat_binary_commit` | string | latest | Chat binary commit SHA |
| `chat_version` | string | - | Chat version for sync |
| `chat_build_timestamp` | string | - | Inherited timestamp |
| `trigger_source` | string | manual | What triggered the build |

**Outputs:**
| Output | Description |
|--------|-------------|
| `version` | Autocomplete version |
| `commit_sha` | Autocomplete commit SHA |
| `branch_name` | Branch name |
| `build_manifest` | JSON manifest with traceability info |
| `build_timestamp` | Build timestamp |

#### 2.2 Release Kiro CLI Non-Prod (release-non-prod.yml)

Renamed from `release-nightly.yml`; handles nightly, beta, and feature releases.

**Triggered by:** chat repo's `dispatch_autocomplete` job (in `build-and-release.yml`)

**Flow:**
```
build (build-kiro-cli.yml) → release_toolbox → GitHub release → notify_slack → summary
```

`notify_slack` uses `CHAT_TRIGGER_TOKEN` to trigger `nightly-release-notification.yml`
back in the chat repo.

#### 2.3 Release to CloudFront (release-cloudfront.yml)

Invokes Lambda to publish artifacts to public CloudFront distribution.

#### 2.4 Release to Toolbox (release-toolbox.yml)

Invokes Lambda to publish artifacts to internal toolbox.

---

## Secrets Reference

### kiro-cli Repository

#### Repository-Level Secrets

| Secret | Purpose | Used By |
|--------|---------|---------|
| `SLACK_WEBHOOK_URL` | Slack notifications for releases | release-notification.yaml |
| `SLACK_NIGHTLY_WEBHOOK_URL` | Slack notifications for nightly | nightly-release-notification.yml |

#### Gamma Environment Secrets

| Secret | Purpose | Used By |
|--------|---------|---------|
| `AUTOCOMPLETE_TRIGGER_TOKEN` | **PAT to trigger autocomplete repo** | build-kiro-cli.yml |
| `AWS_ACCOUNT_ID` | AWS account identifier | build-darwin.yml, build-linux.yml |
| `AWS_GITHUB_ACTIONS_ROLE_ARN` | OIDC role for AWS access | build-darwin.yml, build-linux.yml |
| `BUILD_OUTPUT_BUCKET` | S3 bucket for build artifacts | build-darwin.yml, build-linux.yml |
| `SIGNING_BUCKET_NAME` | S3 bucket for signing | build-darwin.yml |
| `SIGNING_ROLE_ARN` | IAM role for signing | build-darwin.yml |
| `SIGNING_APPLE_NOTARIZING_SECRET_ARN` | Secrets Manager ARN for Apple creds | build-darwin.yml |
| `TOOLBOX_KV2_POC_LAMBDA_ARN` | Lambda for toolbox POC | - |

#### Prod Environment Secrets

| Secret | Purpose |
|--------|---------|
| `AWS_ACCOUNT_ID` | AWS account identifier |
| `AWS_GITHUB_ACTIONS_ROLE_ARN` | OIDC role for AWS access |
| `BUILD_OUTPUT_BUCKET` | S3 bucket for build artifacts |

### kiro-cli-autocomplete Repository

#### Repository-Level Secrets

| Secret | Purpose | Used By |
|--------|---------|---------|
| `CHAT_TRIGGER_TOKEN` | **PAT to trigger chat repo notifications** | release-non-prod.yml |

#### Gamma Environment Secrets

| Secret | Purpose | Used By |
|--------|---------|---------|
| `AWS_ACCOUNT_ID` | AWS account identifier | build-darwin.yml, build-linux.yml |
| `AWS_GITHUB_ACTIONS_ROLE_ARN` | OIDC role for AWS access | All build/release workflows |
| `BUILD_OUTPUT_BUCKET` | S3 bucket for build artifacts | build-darwin.yml, build-linux.yml |
| `SIGNING_BUCKET` | S3 bucket for signing | build-darwin.yml |
| `SIGNING_ROLE_NAME` | IAM role name for signing | build-darwin.yml |
| `APPLE_NOTARIZATION_SECRET_NAME` | Secrets Manager name for Apple creds | build-darwin.yml |
| `PGP_SIGNING_SECRET_ARN` | Secrets Manager ARN for PGP key | build-linux.yml |

#### Gamma-Release Environment Secrets

| Secret | Purpose | Used By |
|--------|---------|---------|
| `AWS_GITHUB_ACTIONS_ROLE_ARN` | OIDC role for release | release-toolbox.yml, release-cloudfront.yml |
| `TOOLBOX_LAMBDA_ARN` | Lambda ARN for toolbox release | release-toolbox.yml |
| `PUBLIC_RELEASE_LAMBDA_ARN` | Lambda ARN for CloudFront release | release-cloudfront.yml |

#### Prod Environment Secrets

Same structure as gamma, pointing to production AWS resources.

#### Prod-Release Environment Secrets

Same structure as gamma-release, pointing to production AWS resources.

---

## Cross-Repository Triggers

### Chat → Autocomplete

**Token:** `AUTOCOMPLETE_TRIGGER_TOKEN` (in kiro-cli gamma environment)

**Location:** `kiro-cli/.github/workflows/build-and-release.yml` (dispatch_autocomplete job)

**Triggers:**
- `release-non-prod.yml` (nightly / beta / feature builds)

**Payload:**
```javascript
{
  release_quality: 'nightly',   // nightly | beta (from stable) | <feature-slug>
  chat_binary_commit: '<commit_sha>',
  chat_version: '2.6.2-nightly.1',
  chat_build_timestamp: '202606120633',
  trigger_source: 'chat-build'
}
```

**Required Token Permissions:**
- Repository: `kiro-team/kiro-cli-autocomplete`
- Permissions: `actions: write`, `contents: read`

### Autocomplete → Chat

**Token:** `CHAT_TRIGGER_TOKEN` (in kiro-cli-autocomplete repository)

**Location:** `kiro-cli-autocomplete/.github/workflows/release-non-prod.yml` (notify_slack job)

**Triggers:**
- `nightly-release-notification.yml`

**Payload:**
```javascript
{
  version: '1.24.2-nightly.4',
  build_timestamp: '202601300633',
  branch_name: 'nightly'
}
```

**Required Token Permissions:**
- Repository: `kiro-team/kiro-cli`
- Permissions: `actions: write`, `contents: read`

---

## Token Management Guide

### Overview of Cross-Repo Tokens

| Token | Location | Purpose | Target Repo |
|-------|----------|---------|-------------|
| `AUTOCOMPLETE_TRIGGER_TOKEN` | kiro-cli (gamma env) | Trigger autocomplete builds after chat build | kiro-cli-autocomplete |
| `CHAT_TRIGGER_TOKEN` | kiro-cli-autocomplete (repo level) | Trigger Slack notification after nightly release | kiro-cli |

### Token Details

#### AUTOCOMPLETE_TRIGGER_TOKEN

| Property | Value |
|----------|-------|
| **Stored In** | `kiro-team/kiro-cli` |
| **Secret Level** | Environment: `gamma` (NOT repo-level) |
| **Used By** | `build-and-release.yml` → dispatch_autocomplete job |
| **Purpose** | Triggers `release-non-prod.yml` in autocomplete repo |
| **Target Repo** | `kiro-team/kiro-cli-autocomplete` |
| **UI Location** | https://github.com/kiro-team/kiro-cli/settings/environments/gamma |

#### CHAT_TRIGGER_TOKEN

| Property | Value |
|----------|-------|
| **Stored In** | `kiro-team/kiro-cli-autocomplete` |
| **Secret Level** | Repository (not environment) |
| **Used By** | `release-non-prod.yml` → notify_slack job |
| **Purpose** | Triggers `nightly-release-notification.yml` for Slack notification |
| **Target Repo** | `kiro-team/kiro-cli` |
| **UI Location** | https://github.com/kiro-team/kiro-cli-autocomplete/settings/secrets/actions |

### How to Regenerate Tokens

#### Step 1: Create a New Personal Access Token

1. Go to https://github.com/settings/tokens
2. Click **"Generate new token"**

**Option A: Fine-grained token (Recommended)**
- Token name: `kiro-cli-cross-repo-trigger` (or descriptive name)
- Expiration: Set as needed (or no expiration)
- Resource owner: `kiro-team`
- Repository access: **Only select repositories**
  - For `AUTOCOMPLETE_TRIGGER_TOKEN`: select `kiro-cli-autocomplete`
  - For `CHAT_TRIGGER_TOKEN`: select `kiro-cli`
- Permissions:
  - `Actions`: **Read and write**
  - `Contents`: **Read-only**
- Click **Generate token**
- **Copy the token immediately** (you won't see it again)

**Option B: Classic token**
- Note: `kiro-cli-cross-repo-trigger`
- Expiration: Set as needed
- Scopes:
  - ✅ `repo` (Full control of private repositories)
  - ✅ `workflow` (Update GitHub Action workflows)
- Click **Generate token**
- **Copy the token immediately**

#### Step 2: Update the Secret

**For AUTOCOMPLETE_TRIGGER_TOKEN:**
```bash
# Must use --env gamma (environment-level, not repo-level!)
gh secret set AUTOCOMPLETE_TRIGGER_TOKEN --repo kiro-team/kiro-cli --env gamma
# Paste token when prompted
```

Or via UI:
1. Go to https://github.com/kiro-team/kiro-cli/settings/environments/gamma
2. Under "Environment secrets", click **Update** on `AUTOCOMPLETE_TRIGGER_TOKEN`
3. Paste the new token

**For CHAT_TRIGGER_TOKEN:**
```bash
# Repo-level secret (no --env flag)
gh secret set CHAT_TRIGGER_TOKEN --repo kiro-team/kiro-cli-autocomplete
# Paste token when prompted
```

Or via UI:
1. Go to https://github.com/kiro-team/kiro-cli-autocomplete/settings/secrets/actions
2. Click **Update** on `CHAT_TRIGGER_TOKEN`
3. Paste the new token

#### Step 3: Verify the Token Works

```bash
# Trigger a nightly build to test both tokens
gh workflow run build-and-release.yml \
  --repo kiro-team/kiro-cli \
  --ref main \
  -f increment=nightly

# Watch the run
gh run list --repo kiro-team/kiro-cli \
  --workflow="Release SOP: 2.0 Build and Trigger Release" --limit 1
```

Check that:
1. `dispatch_autocomplete` job succeeds (tests `AUTOCOMPLETE_TRIGGER_TOKEN`)
2. `notify_slack` job in autocomplete succeeds (tests `CHAT_TRIGGER_TOKEN`)

### Common Token Issues

#### "HttpError: Not Found" (404)

**Cause:** Token doesn't have access to the target repository.

**Check:**
- Token owner is a member of `kiro-team` org
- Token has `Actions: Read and write` permission
- Token is scoped to the correct repository
- For fine-grained tokens: correct repository is selected

#### Token Updated But Still Failing

**Cause:** Secret exists at multiple levels; wrong one being used.

**Check:**
```bash
# AUTOCOMPLETE_TRIGGER_TOKEN must be in gamma environment
gh secret list --repo kiro-team/kiro-cli --env gamma | grep AUTOCOMPLETE

# CHAT_TRIGGER_TOKEN should be at repo level
gh secret list --repo kiro-team/kiro-cli-autocomplete | grep CHAT
```

**Important:** Environment secrets override repo-level secrets. `AUTOCOMPLETE_TRIGGER_TOKEN` MUST be in the gamma environment because the job uses `environment: gamma`.

#### Token Expiration

Check expiration in error logs:
```
'github-authentication-token-expiration': '2027-01-16 14:13:47 -0800'
```

If expired, regenerate following steps above.

### Token Security Best Practices

1. **Use fine-grained tokens** - Scope to specific repos and minimal permissions
2. **Set expiration** - Rotate tokens periodically
3. **Use descriptive names** - Easy to identify which token is which
4. **Document token owner** - Know who created each token for rotation
5. **Monitor for failures** - Token issues show as 404 errors in workflow logs

---

## Manual Triggers

### Trigger Nightly Build (Full Pipeline)

Computes a `-nightly.N` tag, builds chat for all platforms, then dispatches the
autocomplete release → toolbox → Slack notification:

```bash
gh workflow run build-and-release.yml \
  --repo kiro-team/kiro-cli \
  --ref main \
  -f increment=nightly
```

### Build an RC / Stable Release

Run on the release branch created by `cut-release.yml`:

```bash
gh workflow run build-and-release.yml \
  --repo kiro-team/kiro-cli \
  --ref release/2.7.0 \
  -f increment=rc        # or: stable
```

### Dry Run (compute version, do not build)

```bash
gh workflow run build-and-release.yml \
  --repo kiro-team/kiro-cli \
  --ref main \
  -f increment=nightly \
  -f dry_run=true
```

### Trigger Autocomplete Release Directly

Skip the chat build, use an existing chat binary from S3:

```bash
gh workflow run release-non-prod.yml \
  --repo kiro-team/kiro-cli-autocomplete \
  --ref main \
  -f release_quality=nightly \
  -f chat_binary_commit=latest \
  -f trigger_source=manual
```

With a specific chat version:

```bash
gh workflow run release-non-prod.yml \
  --repo kiro-team/kiro-cli-autocomplete \
  --ref main \
  -f release_quality=nightly \
  -f chat_binary_commit=<commit_sha> \
  -f chat_version=2.6.2-nightly.5 \
  -f trigger_source=manual
```

### Check Run Status

```bash
# Get latest orchestrator run
gh run list --repo kiro-team/kiro-cli \
  --workflow="Release SOP: 2.0 Build and Trigger Release" --limit 1

# Anything actively running
gh run list --repo kiro-team/kiro-cli --status=in_progress --limit 10

# Watch / view a run
gh run watch --repo kiro-team/kiro-cli <run_id>
gh run view --repo kiro-team/kiro-cli <run_id>
```

---

## Security Monitoring

### Bundled Bun Runtime

The CLI embeds a Bun binary (version pinned in `scripts/const.py` as `BUN_VERSION`). Since this is a raw binary download and not a package manager dependency, Dependabot cannot track it. Two workflows provide security coverage:

#### Check Bun Version (`check-bun-version.yml`)

Runs daily (9 UTC) and compares the bundled `BUN_VERSION` against the latest release from `oven-sh/bun`. If outdated, it opens a GitHub issue with update instructions. Deduplicates by searching for existing open issues before creating a new one.

**Manual trigger:**
```bash
gh workflow run check-bun-version.yml --repo kiro-team/kiro-cli
```

#### OSV Vulnerability Scan (`osv-scan.yml`)

Runs on PRs, pushes to main, and daily (9 UTC). Scans for known vulnerabilities using [OSV Scanner](https://github.com/google/osv-scanner) across two sources:
- `bun.lock` — JavaScript dependencies
- Custom `osv-scanner-custom.json` — Generated at scan time from `BUN_VERSION` in `scripts/const.py`, registers the bundled Bun binary as `npm/bun` so CVEs against the Bun runtime itself are detected

The scanner runs twice per invocation (it only supports one output format at a time):
1. **Table format** — Human-readable output printed to CI logs. This step fails the workflow if vulnerabilities are found.
2. **SARIF format** — Uploaded to the GitHub Security tab for tracking. Runs with `continue-on-error` since the first step already handles the pass/fail decision.

**Manual trigger:**
```bash
gh workflow run osv-scan.yml --repo kiro-team/kiro-cli
```

**Viewing results:** Check the CI logs for the table output, or go to the repository's Security tab → Code scanning alerts.

### Dependabot

Dependabot is configured in `.github/dependabot.yml` for:
- **github-actions**: Daily updates for workflow action versions
- **cargo**: Daily updates for Rust dependencies (grouped by `aws-*` and `clap*`)
- **npm**: Daily updates for JavaScript dependencies in `packages/tui`

---

## Troubleshooting

### Common Issues

#### 1. Nightly Build Not Triggering Autocomplete

**Symptom:** Chat build succeeds but autocomplete doesn't start.

**Check:**
1. Did both Darwin and Linux builds succeed?
2. Is `AUTOCOMPLETE_TRIGGER_TOKEN` valid in gamma environment?
3. Does the token owner have access to `kiro-cli-autocomplete`?

**Debug:**
```bash
# Check recent chat builds
gh run list --repo kiro-team/kiro-cli --workflow="Release SOP: 2.0 Build and Trigger Release" --limit 5

# Check if autocomplete was triggered
gh run list --repo kiro-team/kiro-cli-autocomplete --workflow=release-non-prod.yml --limit 5

# Check the TriggerAutocompleteBuild job specifically
gh run view <run_id> --repo kiro-team/kiro-cli --log-failed
```

#### 2. Token Expired or Invalid

**Symptom:** `HttpError: Not Found` in TriggerAutocompleteBuild job.

**Diagnosis:**
```bash
# Check token expiration in error message:
# 'github-authentication-token-expiration': '2027-01-16 14:13:47 -0800'

# List current secrets and their update dates
gh secret list --repo kiro-team/kiro-cli --env gamma
```

**Fix:**
1. Create new PAT at https://github.com/settings/tokens
   - Fine-grained: Select `kiro-team/kiro-cli-autocomplete`, grant `Actions: Read and write`
   - Classic: Select `repo` and `workflow` scopes
2. Update the secret (must be in gamma environment, not repo level):
```bash
gh secret set AUTOCOMPLETE_TRIGGER_TOKEN --repo kiro-team/kiro-cli --env gamma
```

**Important:** The token must be in the **gamma environment**, not repository level. Environment secrets override repo secrets.

#### 3. Version Sync Not Working

**Symptom:** Autocomplete version doesn't match chat version.

**Check:**
- Is `chat_version` being passed in the workflow dispatch?
- Check the SyncVersionFromChat job logs.

#### 4. Scheduled Nightly Not Running

**Symptom:** No nightly build at expected time.

**Check:**
```bash
# View scheduled runs
gh run list --repo kiro-team/kiro-cli --workflow="Release SOP: 2.0 Build and Trigger Release" --event=schedule --limit 5

# Check if it was skipped (main did not advance since last nightly tag)
gh run view <run_id> --repo kiro-team/kiro-cli
```

The nightly self-skips (in the `tag` job) if `main` has not advanced since the last nightly tag.

#### 5. Wrong Token Being Used

**Symptom:** Updated token but still getting errors.

**Cause:** Token exists at multiple levels (environment overrides repo).

**Check:**
```bash
# Check repo-level secrets
gh secret list --repo kiro-team/kiro-cli

# Check environment-level secrets (this takes precedence!)
gh secret list --repo kiro-team/kiro-cli --env gamma
```

**Fix:** Update at the correct level:
```bash
# For AUTOCOMPLETE_TRIGGER_TOKEN, must be gamma environment
gh secret set AUTOCOMPLETE_TRIGGER_TOKEN --repo kiro-team/kiro-cli --env gamma
```

#### 4. Build Artifacts Not Found

**Symptom:** Autocomplete build fails downloading chat binary.

**Check:**
- Did chat build upload to S3 successfully?
- Is the `chat_binary_commit` correct?
- Check S3 path: `s3://{bucket}/chat/{branch}/{commit}/{target}/`

### Useful Commands

```bash
# List all workflow runs
gh run list --repo kiro-team/kiro-cli --limit 10
gh run list --repo kiro-team/kiro-cli-autocomplete --limit 10

# View specific run
gh run view <run_id> --repo kiro-team/kiro-cli

# View failed logs
gh run view <run_id> --repo kiro-team/kiro-cli --log-failed

# Manually trigger nightly
gh workflow run build-and-release.yml --repo kiro-team/kiro-cli --ref main -f increment=nightly

# List secrets
gh secret list --repo kiro-team/kiro-cli --env gamma

# Update secret
gh secret set SECRET_NAME --repo kiro-team/kiro-cli --env gamma
```

### Environment Hierarchy

Secrets are resolved in this order (first match wins):
1. **Environment secrets** (e.g., gamma)
2. **Repository secrets**
3. **Organization secrets**

If a secret exists at multiple levels, the environment-level secret takes precedence.

---

## Appendix: Build Targets

### Chat Binary Targets

| Target | Platform | Architecture |
|--------|----------|--------------|
| `universal-apple-darwin` | macOS | x86_64 + aarch64 |
| `x86_64-unknown-linux-gnu` | Linux | x86_64 |
| `x86_64-unknown-linux-musl` | Linux | x86_64 (static) |
| `aarch64-unknown-linux-gnu` | Linux | aarch64 |
| `aarch64-unknown-linux-musl` | Linux | aarch64 (static) |

### Autocomplete Artifacts

| Platform | Artifacts |
|----------|-----------|
| macOS | `.dmg`, `.tar.gz` |
| Linux | `.deb`, `.appimage`, `.tar.gz`, `.tar.zst`, `.tar.xz` |

---

*Last updated: 2026-06-12*
