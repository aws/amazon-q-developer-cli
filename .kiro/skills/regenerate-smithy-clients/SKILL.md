---
name: regenerate-smithy-clients
description: Regenerate the auto-generated amzn Smithy Rust clients. Use when the user asks to update, regenerate, or refresh the internal amzn-* crates (codewhisperer, consolas, qdeveloper-streaming). Triggers on "regenerate clients", "update smithy clients", "generate-clients", "amzn clients".
---

# Regenerate Smithy Clients

Auto-generated internal amzn Rust clients are committed directly to the codebase. When the upstream service API changes, these clients need to be regenerated.

## Affected crates

- `crates/amzn-codewhisperer-client`
- `crates/amzn-codewhisperer-streaming-client`
- `crates/amzn-consolas-client`
- `crates/amzn-qdeveloper-streaming-client`

All are generated from the Brazil package: `AWSVectorConsolasRuntimeServiceRustClient`

## Prerequisites check

Before starting, verify the following:

1. **builder-mcp** — needed to look up the latest version from code.amazon.com. Try a `ReadInternalWebsites` call to verify it's available. If unavailable, tell the user and stop.
2. **mcurl** — midway-authenticated curl, used by the script to download artifacts. Run `which mcurl` to check. If not found, tell the user to install it.
3. **Midway session** — the script uses `mcurl` which requires active midway credentials. Ask the user: "Have you run `mwinit -f` recently?"

Do not proceed until all prerequisites are confirmed.

## Steps

### 1. Look up the latest version

Fetch the releases page to find the latest version:

```
https://code.amazon.com/packages/AWSVectorConsolasRuntimeServiceRustClient/releases
```

Use `ReadInternalWebsites` to read this page. Extract the latest version number (format: `0.1.XXXXX`).

### 2. Create a branch

```bash
git checkout -b update-smithy-clients-<VERSION>
```

### 3. Run the script

Pass the version (without trailing `.0`) as a parameter:

```bash
bash scripts/generate-clients.sh 0.1.XXXXX
```

Prerequisites:
- Active midway session (`mwinit`)
- `mcurl` available
- `cargo +nightly` for formatting

### 4. Fix remaining issues

If `cargo clippy --fix` (run by the script) doesn't resolve all issues:

```bash
cargo clippy --locked --workspace --color always -- -D warnings
```

Manually fix any remaining clippy errors in the regenerated crates.

### 5. Verify build

```bash
cargo build --workspace
```

### 6. Commit and PR

Stage the changed crates **and Cargo.lock**, commit, and open a PR:

```bash
git add Cargo.lock crates/amzn-codewhisperer-client crates/amzn-codewhisperer-streaming-client crates/amzn-consolas-client crates/amzn-qdeveloper-streaming-client
git commit -m "chore: regenerate smithy clients to 0.1.XXXXX"
```



## Notes

- The script downloads `.crate` tarballs from `prod.artifactbrowser.brazil.aws.dev`
- It removes `resolver = "1"` from generated Cargo.toml files (incompatible with workspace)
- The pipeline that builds the client package: https://pipelines.amazon.com/pipelines/AWSVectorConsolasRuntimeServiceRustClient
- `amzn-toolkit-telemetry-client` is currently NOT regenerated (commented out in script)
