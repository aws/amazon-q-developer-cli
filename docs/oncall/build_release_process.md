---
name: build-release-process
description: Kiro CLI build and release infrastructure documentation. Use when debugging build failures, understanding release workflows, checking artifact locations, or triggering releases. Triggers on questions about GitHub Actions, S3 buckets, CloudFront, build environments, or release channels.
---

# Kiro CLI Build & Release Process

> **Last Updated**: December 15, 2025  
> **Status**: Active Development

## Overview

Kiro CLI is built from two repositories:

- **kiro-cli** (chat functionality) - The core chat binary
- **kiro-cli-autocomplete** (this repo) - Autocomplete, desktop app, and build/release infrastructure

This document outlines the complete build and release process, artifact locations, environment configurations, and pending migration tasks.

## Table of Contents

1. [Repository Dependencies](#repository-dependencies)
2. [Environment Configuration](#environment-configuration)
3. [Artifact Locations](#artifact-locations)
4. [Build Process Overview](#build-process-overview)
5. [Release Process Overview](#release-process-overview)
6. [Pending Tasks - Critical Path (P0)](#pending-tasks---critical-path-p0)
7. [Pending Tasks - Testing & Validation](#pending-tasks---testing--validation)
8. [Pending Tasks - P1 Items](#pending-tasks---p1-items)
9. [Quick Reference & Appendix](#quick-reference--appendix)

---

## Repository Dependencies

### Two-Repo Architecture

Kiro CLI is split across two repositories with distinct responsibilities:

| Repository | Purpose | Key Components |
|------------|---------|----------------|
| **kiro-cli** | Chat functionality | Core chat binary (`kiro-cli-chat`) |
| **kiro-cli-autocomplete** | Distribution & UI | Autocomplete, desktop app, build/release infrastructure |

### How They Work Together

#### Binary Integration

**Default Behavior**: Downloads `latest` chat binary from the current branch

**Override Option**: Pin to specific commit via `chat_binary_commit` parameter

**Download Location**: 
```
s3://{BUILD_OUTPUT_BUCKET}/chat/{branch}/{commit|latest}/{target-triple}/kiro-cli-chat.zip
```

**Build Info**: Each chat binary includes `BUILD_INFO.json` with:
- Commit SHA
- Build timestamp

**Build Script**: `build-scripts/main.py` handles download during macOS/Linux builds:
```bash
--chat-build-bucket-name "$BUILD_OUTPUT_BUCKET"
--chat-binary-branch "chat/$BRANCH_NAME"
--chat-binary-commit "$COMMIT_SHA_OR_LATEST"
```

#### Unified Distribution

**Final Artifacts**: Both binaries packaged together:
- macOS: `Kiro CLI.dmg` (contains both binaries)
- Linux: `kirocli.tar.gz` (contains both binaries)

**User Experience**: Single installation provides both autocomplete and chat functionality

---
## Environment Configuration

### Build Environments vs Release Environments

Kiro CLI uses **four distinct GitHub Actions environments** with different purposes and access controls:

| Environment | Purpose | Approval Required | Used By |
|-------------|---------|-------------------|---------|
| **gamma** | Build artifacts in test environment | ❌ No | `build-darwin.yml`, `build-linux.yml` |
| **prod** | Build artifacts in production environment | ❌ No | `build-darwin.yml`, `build-linux.yml` |
| **gamma-release** | Release to gamma public/toolbox | ❌ No | `release-cloudfront.yml`, `release-toolbox.yml` |
| **prod-release** | Release to production public/toolbox | ✅ **Yes** | `release-cloudfront.yml`, `release-toolbox.yml` |

### Build Environments (gamma / prod)

**Purpose**: Compile and sign binaries, upload to build output buckets

**AWS Accounts**:
- Gamma: 230592382359
- Prod: 158872659206

**Secrets**:
- `AWS_GITHUB_ACTIONS_ROLE_ARN` - OIDC role for AWS authentication
- `AWS_ACCOUNT_ID` - AWS account ID
- `BUILD_OUTPUT_BUCKET` - S3 bucket for build artifacts
- `SIGNING_BUCKET` - S3 bucket for signed artifacts
- `SIGNING_ROLE_NAME` - Code signing role
- `PGP_SIGNING_SECRET_ARN` - PGP key for Linux signing
- `APPLE_NOTARIZATION_SECRET_NAME` - Apple notarization credentials

**Workflow Selection**:
```yaml
# Gamma only for feature branches
if [[ "$BRANCH" == "prod" ]] || [[ "$BRANCH" == "main" ]]; then
  ENVIRONMENT=["gamma", "prod"]
else
  ENVIRONMENT=["gamma"]
fi
```

### Release Environments (gamma-release / prod-release)

**Purpose**: Invoke Lambda functions to copy artifacts to public download or toolbox buckets

**Secrets**:
- `AWS_GITHUB_ACTIONS_ROLE_ARN` - OIDC role for AWS authentication
- `PUBLIC_RELEASE_LAMBDA_ARN` - Lambda for public releases
- `TOOLBOX_LAMBDA_ARN` - Lambda for toolbox releases

**Key Difference**: 
- `gamma-release` - Immediate execution, no approval gate
- `prod-release` - **Requires manual approval** before Lambda invocation

**Workflow**: `.github/workflows/release-kiro-cli.yml`
```yaml
environment:
  description: "Deployment environment"
  required: true
  type: choice
  options:
    - gamma-release
    - prod-release
```

### Environment Flow

```
┌─────────────────────────────────────────────────────────────┐
│ 1. BUILD PHASE                                              │
│    Environments: gamma, prod                                │
│    Output: Artifacts in BUILD_OUTPUT_BUCKET                 │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│ 2. RELEASE PHASE                                            │
│    Environments: gamma-release, prod-release                │
│    Output: Artifacts in PUBLIC_DOWNLOAD or TOOLBOX buckets  │
└─────────────────────────────────────────────────────────────┘
```

---
## Artifact Locations

### Migration Overview

Kiro CLI artifacts have been migrated to new S3 buckets and CloudFront distributions with updated naming conventions.

| Environment | Old Bucket/URL | New Bucket/URL |
|-------------|----------------|----------------|
| **Gamma Build Output** | `fig-io-desktop-build-output-230592382359-us-west-2` | `kiro-cli-build-output-gamma-us-east-1-230592382359` |
| **Gamma Public Download** | `fig-io-desktop-gamma-us-east-1-download` | `kiro-cli-public-download-gamma-us-east-1-230592382359` |
| **Gamma CloudFront** | `https://desktop.gamma-us-east-1.codewhisperer.ai.aws.dev` | `https://download.gamma.cli.kiro.dev` |
| **Prod Build Output** | `fig-io-desktop-build-output-158872659206-us-west-2` | `kiro-cli-build-output-prod-us-east-1-158872659206` |
| **Prod Public Download** | `fig-io-desktop-prod-us-east-1-download` | `kiro-cli-public-download-prod-us-east-1-158872659206` |
| **Prod CloudFront** | Similar structure to gamma | `https://prod.download.cli.kiro.dev` |

**Cloudfront VPN Requirements**:
- Gamma: ✅ Required
- Prod: ❌ Not required

---

### Gamma Environment (Account: 230592382359)

#### Build Output Bucket
**Bucket**: `kiro-cli-build-output-gamma-us-east-1-230592382359`  
**Region**: `us-east-1`
**IsenLink**: https://tiny.amazon.com/icf8ulfx/IsenLink

```
s3://kiro-cli-build-output-gamma-us-east-1-230592382359/
├── autocomplete/
│   └── {branch}/
│       └── {commit}/
│           ├── macos/
│           │   ├── Kiro CLI.dmg
│           │   ├── Kiro CLI.dmg.sha256
│           │   └── Kiro CLI.tar.gz
│           └── {target-triple}/
│               ├── kirocli.tar.gz
│               ├── kirocli.tar.gz.sha256
│               ├── kirocli.tar.gz.sig
│               └── kirocli.tar.gz.asc
└── chat/
    └── {branch}/
        ├── {commit}/
        │   └── {target-triple}/
        │       ├── kiro-cli-chat.zip (contains BUILD_INFO.json)
        │       └── kiro-cli-chat.zip.sha256
        └── latest/
            └── {target-triple}/
                ├── kiro-cli-chat.zip (contains BUILD_INFO.json)
                └── kiro-cli-chat.zip.sha256
```

#### Public Download Bucket
**Bucket**: `kiro-cli-public-download-gamma-us-east-1-230592382359`
**IsenLink**: https://tiny.amazon.com/ayklalm3/IsenLink
**CloudFront**: `https://download.gamma.cli.kiro.dev` (VPN required)

```
s3://kiro-cli-public-download-gamma-us-east-1-230592382359/
├── stable/
│   ├── index.json
│   ├── {version}/
│   │   └── (all platform artifacts)
│   └── latest/
│       ├── manifest.json
│       └── (all platform artifacts)
├── nightly/
│   └── (same structure)
└── insider/
    └── (same structure)
```

#### Signed Artifacts (Intermediate)
**Bucket**: `kiro-cli-signed-artifacts-gamma-us-east-1-230592382359`
**IsenLink**: https://tiny.amazon.com/dziltx9a/IsenLink

Used temporarily during code signing process.

#### Install Scripts
**Bucket**: `kiro-cli-install-scripts-gamma-us-east-1-230592382359`
**IsenLink**: https://tiny.amazon.com/fyno32wm/IsenLink

Hosts `install.sh` and deployment scripts.

---

### Prod Environment (Account: 158872659206)

#### Build Output Bucket
**Bucket**: `kiro-cli-build-output-prod-us-east-1-158872659206`  
**Region**: `us-east-1`
**IsenLink**: https://tiny.amazon.com/vj5e82f6/IsenLink

Structure identical to Gamma build output bucket.

#### Public Download Bucket
**Bucket**: `kiro-cli-public-download-prod-us-east-1-158872659206`  
**IsenLink**: https://tiny.amazon.com/1fa652ng3/IsenLink
**CloudFront**: `https://prod.download.cli.kiro.dev` (No VPN required)

Structure identical to Gamma public download bucket.

#### Signed Artifacts (Intermediate)
**Bucket**: `kiro-cli-signed-artifacts-prod-us-east-1-158872659206`
**IsenLink**: https://tiny.amazon.com/o69mgiuf/IsenLink

#### Install Scripts
**Bucket**: `kiro-cli-install-scripts-prod-us-east-1-158872659206`
**IsenLink**: https://tiny.amazon.com/1fwlb452l/IsenLink

---

### Toolbox Buckets (Internal Amazon)

**Account**: 211125606403  
**Bucket**: `buildertoolbox-kiro-cli-us-west-2`  
**Region**: `us-west-2`
**IsenLink**: https://tiny.amazon.com/ym2djmdm/IsenLink

Used for internal Amazon toolbox distribution.

---
## Build Process Overview

### Build Triggers

The autocomplete repo supports three types of build triggers:

#### 1. Automated Branch Builds
**Trigger**: Push to `main`, `feature/*`, `prod` branches  
**Type**: Insider build (automatic)  
**Version**: No increment (`none`) - keeps current version

#### 2. Scheduled Nightly Builds
**Trigger**: Daily at 06:21 UTC (`21 6 * * *`)  
**Condition**: Only runs on `main` branch  
**Type**: Nightly build  
**Version**: Patch increment
- Example: `1.2.3` → `1.2.4`

#### 3. Manual Builds
**Trigger**: `workflow_dispatch` via GitHub Actions UI  
**Options**:
- **Version Increment**: 
  - `patch`: `1.2.3` → `1.2.4`
  - `minor`: `1.2.3` → `1.3.0`
  - `major`: `1.2.3` → `2.0.0`
  - `none`: Keep current version
- **Release Quality**: `stable`, `nightly`, `insider`
- **Chat Binary Commit**: Specific SHA or `latest` (default)
- **Environment**: Gamma (default), Prod (for `main`/`prod` branches)

---

### Build Workflow

**Main Workflow**: `.github/workflows/build-kiro-cli.yml`

```
┌─────────────────────────────────────────────────────────────┐
│ 1. ValidateReleaseTrigger                                   │
│    - Determine if build should proceed                      │
│    - Set release type (stable/nightly/insider)              │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│ 2. SyncVersionFromChat                                      │
│    - Fetch chat repo version from Cargo.toml                │
│    - Update version if version_increment != 'none'          │
│    - Determine environment (gamma only or gamma+prod)       │
│    - Generate build timestamp                               │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│ 3. TriggerDarwinBuild + TriggerLinuxBuild (parallel)        │
│    - Invoke platform-specific build workflows               │
│    - Pass: version, timestamp, channel, chat_binary_commit  │
└─────────────────────────────────────────────────────────────┘
```

---

### Platform-Specific Builds

#### Darwin (macOS) Build
**Workflow**: `.github/workflows/build-darwin.yml`

**Outputs**:
- `Kiro CLI.dmg` (signed & notarized universal binary)
- `Kiro CLI.dmg.sha256`
- `Kiro CLI.tar.gz`

**Key Steps**:
1. Setup Rust with dual targets (`x86_64-apple-darwin`, `aarch64-apple-darwin`)
2. Install tauri-cli v1.6.0
3. Download chat binary from S3 (commit-specific or `latest`)
4. Run `build-scripts/main.py build` with signing parameters
5. Code sign with AWS signing service
6. Notarize with Apple
7. Upload to: `s3://{BUILD_OUTPUT_BUCKET}/autocomplete/{branch}/{commit}/macos/`

#### Linux Build
**Workflow**: `.github/workflows/build-linux.yml`

**Build Matrix**:
- `x86_64-unknown-linux-gnu` (full + minimal variants)
- `x86_64-unknown-linux-musl` (minimal)
- `aarch64-unknown-linux-gnu` (minimal)
- `aarch64-unknown-linux-musl` (minimal)

**Outputs per target**:
- `.tar.gz`, `.tar.zst`, `.tar.xz` (compressed archives)
- `.deb` (Debian package)
- `.appimage` (AppImage for full variant only)
- `.sha256`, `.sig`, `.asc` (checksums and signatures)

**Key Steps**:
1. Install system dependencies (GTK, WebKit, etc.)
2. Setup Rust and cross-compilation tools
3. Download chat binary from S3 (commit-specific or `latest`)
4. Run `build-scripts/main.py build` with variant flags
5. PGP sign artifacts
6. Upload to: `s3://{BUILD_OUTPUT_BUCKET}/autocomplete/{branch}/{commit}/{target}/`

---

### Chat Repo Build Process

**Repository**: `kiro-cli` (separate repo)  
**Workflow**: `.github/workflows/release-kiro-cli.yml`  
**Schedule**: Daily at 05:21 UTC - runs **1 hour before** autocomplete builds

**Outputs**: Chat binaries uploaded to:
```
s3://{BUILD_OUTPUT_BUCKET}/chat/{branch}/{commit}/{target}/kiro-cli-chat.zip
s3://{BUILD_OUTPUT_BUCKET}/chat/{branch}/latest/{target}/kiro-cli-chat.zip
```

**Integration**: Autocomplete builds download from `latest` path by default, or specific commit if pinned

---
## Release Process Overview

### Release Channels

Kiro CLI supports three release channels with different stability levels and caching policies:

| Channel | Path Prefix | Cache TTL | Use Case |
|---------|-------------|-----------|----------|
| **stable** | `/stable/` | 7 days (max 30) | Production releases (Prod only) |
| **nightly** | `/nightly/` | 1 hour (max 12) | Daily development builds |
| **insider** | `/insider/` | 6 hours (max 1 day) | Pre-release testing, feature branches |

---

### Release Workflows

#### Production Release (Recommended)

**Workflow**: `.github/workflows/release-kiro-cli-prod.yml`

**Trigger**: Manual `workflow_dispatch` only

**Purpose**: Streamlined workflow for production releases with minimal configuration

**Required Inputs**:
- `commit`: Git commit SHA
- `version`: Version string (e.g., `1.2.3`)
- `channel`: Release channel (currently only `stable` enabled)

**Optional Inputs**:
- `release_to_toolbox`: Also release to internal toolbox (default: `false`)

**Hardcoded Settings**:
- Branch: `prod`
- Environment: `prod-release` (requires manual approval)
- Public release: Always enabled
- No bucket overrides or advanced options

---

#### Advanced Release (All Options)

**Workflow**: `.github/workflows/release-kiro-cli.yml`

**Trigger**: Manual `workflow_dispatch` only

**Purpose**: Unified workflow with full control over release options (for gamma/testing)

**Required Inputs**:
- `commit`: Git commit SHA
- `version`: Version string (e.g., `1.2.3`)
- `channel`: Release channel (`stable`, `nightly`, `insider`)
- `branch_name`: Branch name
- `environment`: `gamma-release` or `prod-release`

**Optional Inputs**:
- `release_to_cloudfront`: Enable public release (default: `true`)
- `release_to_toolbox`: Enable toolbox release (default: `false`)
- `overwrite_existing_entry`: Overwrite existing version (public only)
- `dst_bucket_override`: Override destination bucket (public only)
- `update_conditions`: Update conditions JSON (public only)
- `override_channel_check`: Override channel validation (toolbox only)

---

### Release Workflows

Both public and toolbox release workflows support `workflow_call` (reusable), invoked by the release workflows above:

#### 1. Public Release
**Workflow**: `.github/workflows/release-cloudfront.yml`

**Process**:
1. Download artifacts from build output bucket
2. Validate commit, version, and channel
3. Invoke Lambda to copy artifacts to public download bucket
4. Update channel-specific `index.json`
5. Copy artifacts to `/{channel}/{version}/`
6. Copy artifacts to `/{channel}/latest/` with `manifest.json`
7. Invalidate CloudFront cache

**Environments**:
- `gamma-release` - No approval required
- `prod-release` - **Requires manual approval**

#### 2. Toolbox Release
**Workflow**: `.github/workflows/release-toolbox.yml`

**Platforms**:
- `alinux` (x86_64-unknown-linux-musl)
- `alinux_aarch64` (aarch64-unknown-linux-musl)
- `ubuntu` (x86_64-unknown-linux-gnu)
- `osx` (universal binary)

**Process**:
1. Download artifacts from build output bucket
2. Extract and bundle with toolbox metadata
3. Invoke Lambda to run `toolbox-bundler`
4. Run `toolbox-publisher` to upload to toolbox S3 buckets

**Environments**:
- `gamma-release` - No approval required
- `prod-release` - **Requires manual approval**

---

### Release Flow Diagram

```
┌─────────────────────────────────────────────────────────────┐
│ Build Complete                                              │
│ Artifacts in: s3://{BUILD_OUTPUT_BUCKET}/autocomplete/...   │
└─────────────────────────────────────────────────────────────┘
                            ↓
┌─────────────────────────────────────────────────────────────┐
│ Manual Trigger: release-kiro-cli.yml                        │
│ Inputs: commit, version, channel, environment               │
│ Options: release_to_cloudfront, release_to_toolbox          │
└─────────────────────────────────────────────────────────────┘
                            ↓
        ┌───────────────────┴───────────────────┐
        ↓                                       ↓
┌──────────────────┐                  ┌──────────────────┐
│ Public Release   │                  │ Toolbox Release  │
│ (if enabled)     │                  │ (if enabled)     │
└──────────────────┘                  └──────────────────┘
        ↓                                       ↓
┌──────────────────┐                  ┌──────────────────┐
│ Lambda invoked   │                  │ Lambda invoked   │
│ Copy to public   │                  │ Package for      │
│ download bucket  │                  │ toolbox          │
└──────────────────┘                  └──────────────────┘
        ↓                                       ↓
┌──────────────────┐                  ┌──────────────────┐
│ CloudFront       │                  │ Toolbox S3       │
│ invalidation     │                  │ upload           │
└──────────────────┘                  └──────────────────┘
```

---
## Pending Tasks - Critical Path (P0)

These are **blocking tasks** that must be completed before the new infrastructure can be fully operational.

### Task 1: Update Install Script BASE_URL

**Priority**: 🔴 P0 - Critical  
**Status**: ⏳ Pending

**File**: `scripts/install.sh`

**Current State**:
```bash
SCRIPT_URL="{{SCRIPT_URL}}"
BASE_URL="{{BASE_URL}}"
MANIFEST_URL="${BASE_URL}/latest/manifest.json"
```

**Required Changes**:
- Update `{{BASE_URL}}` template placeholder to point to new CloudFront URLs:
  - **Gamma**: `https://download.gamma.cli.kiro.dev`
  - **Prod**: `https://prod.download.cli.kiro.dev`
- Update `{{SCRIPT_URL}}` template placeholder if needed

**Impact**: Users running the install script will fail to download artifacts until this is updated

**Related Workflow**: `scripts/deploy_install_script.py` handles deployment to install scripts bucket

---

### Task 2: Update fig_install Download Bucket

**Priority**: 🔴 P0 - Critical  
**Status**: ⏳ Pending

**File**: `crates/fig_install/src/index.rs`

**Current State**:
```rust
const DEFAULT_RELEASE_URL: &str = "https://desktop-release.q.us-east-1.amazonaws.com";
```

**Required Changes**:
- Update `DEFAULT_RELEASE_URL` to new production CloudFront URL:
  ```rust
  const DEFAULT_RELEASE_URL: &str = "https://prod.download.cli.kiro.dev";
  ```

**Fallback Behavior**: The code checks in this order:
1. Environment variable: `Q_DESKTOP_RELEASE_URL`
2. Setting: `install.releaseUrl`
3. Build-time env var: `Q_BUILD_DESKTOP_RELEASE_URL`
4. Hardcoded default: `DEFAULT_RELEASE_URL`

**Impact**: Auto-update functionality will fail to check for new versions until this is updated

**Testing**: Verify update checks work after change:
```bash
# Set env var for testing
export Q_DESKTOP_RELEASE_URL="https://download.gamma.cli.kiro.dev"
kiro-cli update check
```

---

### Validation Checklist

Complete these steps in order:

1. **Code Changes**
   - [ ] Update `scripts/install.sh` with new BASE_URL values
   - [ ] Update `crates/fig_install/src/index.rs` with new DEFAULT_RELEASE_URL
   - [ ] Commit and push changes

2. **Build**
   - [ ] Trigger build workflow with updated code
   - [ ] Verify build completes successfully for all platforms

3. **Release**
   - [ ] Release new version to gamma environment
   - [ ] Release new version to prod environment

4. **Deploy Install Scripts**
   - [ ] Deploy install script to gamma install scripts bucket
   - [ ] Deploy install script to prod install scripts bucket

5. **Validation**
   - [ ] Test manual installation from gamma CloudFront URL
   - [ ] Test manual installation from prod CloudFront URL
   - [ ] Test auto-update functionality in gamma environment
   - [ ] Test auto-update functionality in prod environment
   - [ ] Verify `manifest.json` is accessible at `{BASE_URL}/latest/manifest.json`

---
## Pending Tasks - Testing & Validation

### Artifact Testing Matrix

The following table tracks testing status across different platforms and environments:

| Platform | Architecture | Build Variant | Status | Notes |
|----------|--------------|---------------|--------|-------|
| **macOS** | Universal (x86_64 + aarch64) | Full | ✅ Tested | DMG installation and app launch verified |
| **Dev Desktop** | x86_64 | musl (minimal) | ✅ Tested | Verified on internal dev-desktop environment |
| **Dev Desktop** | aarch64 | musl (minimal) | ✅ Tested | Verified on internal dev-desktop environment |
| **Ubuntu** | x86_64 | gnu (full) | ⏳ Pending | Needs validation after recent fixes |
| **Ubuntu** | x86_64 | gnu (minimal) | ⏳ Pending | Needs validation after recent fixes |
| **Ubuntu** | aarch64 | gnu (minimal) | ⏳ Pending | Needs validation after recent fixes |
| **WSL (Ubuntu)** | x86_64 | gnu (full) | ❌ Not Tested | Requires testing on Windows Subsystem for Linux |
| **WSL (Ubuntu)** | x86_64 | gnu (minimal) | ❌ Not Tested | Requires testing on Windows Subsystem for Linux |

### Testing Checklist

For each platform, verify the following:

#### Installation Testing
- [ ] Download artifact from CloudFront URL
- [ ] Verify checksum (`.sha256` file)
- [ ] Extract/install package successfully
- [ ] Verify both `kiro-cli` and `kiro-cli-chat` binaries are present
- [ ] Check binary permissions and executability

#### Functional Testing
- [ ] Run `kiro-cli --version` successfully
- [ ] Run `kiro-cli chat` and verify chat functionality
- [ ] Test autocomplete integration in shell
- [ ] Verify desktop app launches (macOS only)
- [ ] Test auto-update check: `kiro-cli update check`

#### Environment-Specific Testing
- [ ] Test in clean environment (no previous installation)
- [ ] Test upgrade from previous version
- [ ] Verify uninstall process

### Known Issues

Document any platform-specific issues discovered during testing:

- **Ubuntu**: Recent fixes applied, needs re-validation
- **WSL**: Not yet tested

### Testing Resources

- **Gamma Artifacts**: `https://download.gamma.cli.kiro.dev` (VPN required)
- **Prod Artifacts**: `https://prod.download.cli.kiro.dev`
- **Install Script**: Use updated `scripts/install.sh` after P0 tasks complete

---

## Pending Tasks - P1 Items

These are **important but non-blocking** tasks that should be completed to improve operational efficiency.

### Task 1: Cleanup Script for Old Builds

**Priority**: 🟡 P1 - Important  
**Status**: ⏳ Pending

**Problem**: Build artifacts accumulate in S3 buckets over time, increasing storage costs and making it harder to find recent builds.

**Proposed Solution**: Create automated cleanup workflow or script

**Requirements**:
- Retain builds from `main` and `prod` branches for longer period (e.g., 90 days)
- Retain feature branch builds for shorter period (e.g., 30 days)
- Always preserve builds that have been released to public/toolbox
- Provide dry-run mode to preview deletions
- Log all deletions for audit trail

**Suggested Implementation**:
- GitHub Actions workflow scheduled to run weekly
- Query S3 buckets for artifacts older than retention period
- Cross-reference with release history to avoid deleting released builds
- Delete old artifacts and generate summary report

**Affected Buckets**:
- `kiro-cli-build-output-gamma-us-east-1-230592382359`
- `kiro-cli-build-output-prod-us-east-1-158872659206`
- `kiro-cli-signed-artifacts-gamma-us-east-1-230592382359`
- `kiro-cli-signed-artifacts-prod-us-east-1-158872659206`

---

### Task 2: Build Optimization Investigation

**Priority**: 🟡 P1 - Important  
**Status**: ⏳ Pending

**Problem**: Build times can be lengthy, especially for full matrix builds across all platforms and architectures.

**Areas to Investigate**:

1. **Caching Improvements**
   - Review Rust cargo cache effectiveness
   - Evaluate npm/pnpm cache strategies
   - Consider caching compiled dependencies

2. **Parallel Build Optimization**
   - Analyze current parallelization strategy
   - Identify bottlenecks in build matrix
   - Consider splitting large jobs into smaller parallel tasks

3. **Dependency Management**
   - Review unnecessary dependencies
   - Evaluate build-time vs runtime dependencies
   - Consider pre-built binaries for heavy dependencies

4. **Build Script Refactoring**
   - Break down `build-scripts/main.py` into individual steps:
     - Separate build step
     - Separate signing step
     - Separate notarization step (macOS)
   - Enable profiling of each step independently
   - Allow running individual steps for debugging
   - Make it easier to identify which step is the bottleneck

---

### Task 3: Rollback Procedures

**Priority**: 🟡 P1 - Important  
**Status**: ⏳ Pending (from Section 5)

**Problem**: No documented process for rolling back a bad release.

**Requirements**:
- Document steps to revert to previous version
- Identify which artifacts need to be restored
- Define CloudFront cache invalidation strategy
- Create rollback checklist for on-call engineers

**Suggested Documentation**:
- Emergency rollback runbook
- Version history tracking
- Communication plan for users

---
## Quick Reference & Appendix

### Quick Command Reference

#### Trigger a Manual Build
```bash
# Navigate to: https://github.com/kiro-team/kiro-cli-autocomplete/actions/workflows/build-kiro-cli.yml
# Click "Run workflow"
# Select:
#   - Branch: main (or feature branch)
#   - Version increment: patch/minor/major/none
#   - Release quality: stable/nightly/insider
#   - Chat binary commit: latest (or specific SHA)
```

#### Trigger a Release

**Production Release (Recommended)**:
```bash
# Navigate to: https://github.com/kiro-team/kiro-cli-autocomplete/actions/workflows/release-kiro-cli-prod.yml
# Click "Run workflow"
# Required inputs:
#   - commit: Git commit SHA from build
#   - version: Version string (e.g., 1.2.3)
#   - channel: stable (only option currently)
# Optional:
#   - release_to_toolbox: true/false
```

**Advanced Release (All Options)**:
```bash
# Navigate to: https://github.com/kiro-team/kiro-cli-autocomplete/actions/workflows/release-kiro-cli.yml
# Click "Run workflow"
# Required inputs:
#   - commit: Git commit SHA from build
#   - version: Version string (e.g., 1.2.3)
#   - channel: stable/nightly/insider
#   - branch_name: Branch name
#   - environment: gamma-release or prod-release
# Optional:
#   - release_to_cloudfront: true/false
#   - release_to_toolbox: true/false
```

#### Check Build Artifacts
```bash
# Gamma
aws s3 ls s3://kiro-cli-build-output-gamma-us-east-1-230592382359/autocomplete/main/ --profile gamma

# Prod
aws s3 ls s3://kiro-cli-build-output-prod-us-east-1-158872659206/autocomplete/main/ --profile prod
```

#### Test Installation from Gamma
```bash
# Download install script
curl -fsSL https://gamma.cli.kiro.dev/install | bash

# Or with specific channel
curl -fsSL https://gamma.cli.kiro.dev/install | bash -s -- --channel nightly
```

---

### Important Links

#### Repositories
- **Autocomplete Repo**: https://github.com/kiro-team/kiro-cli-autocomplete
- **Chat Repo**: https://github.com/kiro-team/kiro-cli

#### GitHub Actions Workflows
- **Build Workflow**: https://github.com/kiro-team/kiro-cli-autocomplete/actions/workflows/build-kiro-cli.yml
- **Release Workflow (Prod)**: https://github.com/kiro-team/kiro-cli-autocomplete/actions/workflows/release-kiro-cli-prod.yml
- **Release Workflow (Advanced)**: https://github.com/kiro-team/kiro-cli-autocomplete/actions/workflows/release-kiro-cli.yml
- **Darwin Build**: https://github.com/kiro-team/kiro-cli-autocomplete/actions/workflows/build-darwin.yml
- **Linux Build**: https://github.com/kiro-team/kiro-cli-autocomplete/actions/workflows/build-linux.yml

#### Infrastructure
- **CDK Repo**: https://code.amazon.com/packages/KiroCliDeployCDK
- **Lambda Repo**: https://code.amazon.com/packages/KiroCliDeployLambda
- **Deployment Pipeline**: https://pipelines.amazon.com/pipelines/KiroCliDeploy

#### CloudFront Distributions
- **Gamma Download**: https://download.gamma.cli.kiro.dev (VPN required)
- **Prod Download**: https://prod.download.cli.kiro.dev

#### S3 Buckets (Gamma - 230592382359)
- Build Output: `kiro-cli-build-output-gamma-us-east-1-230592382359`
- Public Download: `kiro-cli-public-download-gamma-us-east-1-230592382359`
- Signed Artifacts: `kiro-cli-signed-artifacts-gamma-us-east-1-230592382359`
- Install Scripts: `kiro-cli-install-scripts-gamma-us-east-1-230592382359`

#### S3 Buckets (Prod - 158872659206)
- Build Output: `kiro-cli-build-output-prod-us-east-1-158872659206`
- Public Download: `kiro-cli-public-download-prod-us-east-1-158872659206`
- Signed Artifacts: `kiro-cli-signed-artifacts-prod-us-east-1-158872659206`
- Install Scripts: `kiro-cli-install-scripts-prod-us-east-1-158872659206`

#### Toolbox
- Bucket: `buildertoolbox-kiro-cli-us-west-2` (Account: 211125606403)

---

### Troubleshooting

#### Build Failures

**Problem**: Darwin build fails during notarization  
**Solution**: Check Apple ID credentials in Secrets Manager, verify certificate validity

**Problem**: Linux build fails with missing dependencies  
**Solution**: Review system dependencies in `build-linux.yml`, ensure all required packages are installed

**Problem**: Chat binary download fails  
**Solution**: Verify chat repo has completed its build, check S3 bucket permissions

#### Release Failures

**Problem**: Lambda invocation times out  
**Solution**: Check Lambda logs in CloudWatch, verify artifacts exist in source bucket

**Problem**: CloudFront invalidation fails  
**Solution**: Verify CloudFront distribution ID, check IAM permissions for invalidation

**Problem**: Artifacts not appearing at CloudFront URL  
**Solution**: Wait for CloudFront propagation (can take 5-15 minutes), check S3 bucket sync

#### Installation Issues

**Problem**: Install script fails to download artifacts  
**Solution**: Verify BASE_URL is correct, check CloudFront distribution is active, ensure VPN connection for gamma

**Problem**: Auto-update check fails  
**Solution**: Verify `DEFAULT_RELEASE_URL` in `fig_install/src/index.rs` is correct

---

### Related Documentation

- [Main README](README.md) - Project overview and setup
- [Codebase Summary](codebase-summary.md) - Architecture and components
- [Contributing Guide](CONTRIBUTING.md) - How to contribute to the project

