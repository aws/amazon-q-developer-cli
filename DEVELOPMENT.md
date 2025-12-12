# DEVELOPMENT.md

## Overview

Kiro CLI is a Rust-based command-line interface for AWS Q Developer chat functionality.

**Repository**: https://github.com/kiro-team/kiro-cli

---

## Build & Release Process

### Automated Build Triggers

#### 1. Branch Auto-Build
- **Trigger**: Push to `main`, `feature/*`, `build-infra` branches
- **Workflow**: `.github/workflows/release-kiro-cli.yml`
- **Type**: Insider build (no version increment)

#### 2. Scheduled Nightly Builds
- **Trigger**: Daily at 05:21 UTC (`21 5 * * *`)
- **Condition**: Only on `main` branch
- **Type**: Nightly build with patch version increment

#### 3. Manual Release
- **Trigger**: `workflow_dispatch` via GitHub Actions
- **Options**:
  - Version increment: `patch`, `minor`, `major`, `none`
  - Release quality: `stable`, `nightly`, `insider`
  - Create release branch: `true`/`false` (stable releases only)
- **Environments**: Gamma (default), Prod (for production releases)

### Build Workflows

#### Darwin (macOS) Build
**File**: `.github/workflows/build-darwin.yml`

**Targets**:
- `x86_64-apple-darwin`
- `aarch64-apple-darwin`
- Universal binary (combined)

**Process**:
1. Setup Rust with dual targets
2. Install Python dependencies from `scripts/requirements.txt`
3. Run `python3 ./scripts/main.py build --skip-lints --skip-tests`
4. Upload artifacts to S3: `s3://{BUILD_OUTPUT_BUCKET}/chat/{branch}/{commit}/`

#### Linux Build
**File**: `.github/workflows/build-linux.yml`

**Build Matrix**:
- `x86_64-unknown-linux-gnu`
- `x86_64-unknown-linux-musl`
- `aarch64-unknown-linux-gnu`
- `aarch64-unknown-linux-musl`

**Process**:
1. Install Python dependencies
2. Setup Rust and cross-compilation tools
3. Run `python3 ./scripts/main.py build`
4. Upload artifacts to S3: `s3://{BUILD_OUTPUT_BUCKET}/chat/{branch}/{commit}/{target}/`

### Release Channels

| Channel | Use Case |
|---------|----------|
| **stable** | Production releases |
| **nightly** | Daily development builds |
| **insider** | Feature branch and auto-builds |

---

## CI/CD Workflows

### Rust CI
**File**: `.github/workflows/rust.yml`

**Jobs**:
- **cargo-clippy**: Linting on Ubuntu and macOS
- **cargo-test**: Unit tests with code coverage
- **cargo-fmt**: Code formatting check (nightly)
- **cargo-deny**: License and security checks

**Triggers**: Push to any branch, manual dispatch

**Dependencies** (Linux):
```bash
build-essential pkg-config jq dpkg curl wget zstd cmake clang libssl-dev 
libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libdbus-1-dev 
libwebkit2gtk-4.1-dev libjavascriptcoregtk-4.1-dev valac libibus-1.0-dev 
libglib2.0-dev sqlite3 libxdo-dev protobuf-compiler libfuse2 bash fish zsh shellcheck
```

**Dependencies** (macOS):
```bash
protobuf fish shellcheck
```

### Other Workflows

#### mdbook
**File**: `.github/workflows/mdbook.yml`  
**Purpose**: Build and deploy documentation

#### typos
**File**: `.github/workflows/typos.yml`  
**Purpose**: Spell checking

#### terminal-bench
**File**: `.github/workflows/terminal-bench.yaml`  
**Purpose**: Performance benchmarking

#### check-merge-conflicts
**File**: `.github/workflows/check-merge-conflicts.yml`  
**Purpose**: Validate merge conflict markers

#### release-notification
**File**: `.github/workflows/release-notification.yaml`  
**Purpose**: Notify on releases

---

## S3 Artifact Locations

### Build Output Structure
```
s3://{BUILD_OUTPUT_BUCKET}/
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

**BUILD_INFO.json** contains:
- Commit SHA
- Build timestamp
- Version information

---

## Environment Configuration

### GitHub Actions Environments

#### Build Environments (gamma/prod)

**Secrets**:

| Secret | Description |
|--------|-------------|
| `AWS_GITHUB_ACTIONS_ROLE_ARN` | OIDC role ARN for AWS authentication |
| `AWS_ACCOUNT_ID` | AWS account ID |
| `BUILD_OUTPUT_BUCKET` | Build artifacts bucket name |

---

## Local Development

### Project Structure

**Workspace Members**:
- `crates/chat-cli` - Main CLI binary
- `crates/chat-cli-ui` - UI components
- `crates/agent` - Agent functionality
- `crates/code-agent-sdk` - Code agent SDK
- `crates/amzn-codewhisperer-client` - CodeWhisperer API client
- `crates/amzn-codewhisperer-streaming-client` - Streaming client
- `crates/amzn-consolas-client` - Consolas API client
- `crates/amzn-qdeveloper-streaming-client` - Q Developer streaming
- `crates/amzn-toolkit-telemetry-client` - Telemetry client
- `crates/aws-toolkit-telemetry-definitions` - Telemetry definitions
- `crates/semantic-search-client` - Semantic search client

---

## Testing Release Process

1. Push to feature branch Or Manually trigger `release-kiro-cli.yml` workflow
2. Select environment (gamma for testing)
3. Verify artifacts in S3
4. Test binary from S3 location

---

## Related Documentation

- [README](README.md) - Installation and usage
- [SECURITY](SECURITY.md) - Security policies
