# RC Rust Binary Reuse

## Decision

RC Certification may reuse an asset-free `chat_cli` executable across workflow
runs only when a canonical build identity has an exact GitHub Actions cache hit.
The identity, cached bytes, and per-run artifact are independently validated
before a test process can use the executable.

This is an optimization, not a source of truth. A cache miss, eviction, or
service error runs the normal Cargo build. An exact hit with invalid contents
fails closed because rebuilding would hide corruption under an occupied,
immutable key.

## Why the Commit SHA Is Insufficient

The executable does not depend on most TUI, scenario, or documentation changes,
so `github.sha` would discard safe reuse. Conversely, source alone is
insufficient because Cargo output can change with build scripts, environment
variables, toolchains, native linkers, and hosted runner images.

The cache key is therefore the SHA-256 of a canonical identity:

| Input              | Included values                                                                                                                                                        | Reason                                                                                                                                           |
| ------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------ |
| Rust source        | Git index entries and checked-out SHA-256 content for `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`, `.cargo/**`, `crates/**`, `autodocs/**`, and `autodocs-v2/**` | Covers workspace code, manifests, lockfile, build scripts, configuration, platform checkout transformations, and files embedded with `include_*` |
| Cache/build recipe | RC workflow, cache entrypoint, and cache module directory Git index entries                                                                                            | Invalidates reuse when the recipe or validation rules change                                                                                     |
| Runner             | Label, OS, architecture, image OS/version, and OS release                                                                                                              | Prevents cross-platform or silently updated image reuse                                                                                          |
| Toolchain          | Exact successful version stream from Rust, Cargo, native compiler/linker, SDK, and required native dependency and package-manager revision probes                      | Captures tools and installed package revisions that can change generated or linked bytes; missing native metadata disables reuse                 |
| Build plan         | Executable, package, profile, feature arguments, output name, and output path                                                                                          | Provides one typed contract that both identifies and executes the Linux/macOS or Windows build                                                   |
| Environment        | Version/build metadata, Rust flags, Cargo target/profile settings, compiler/linker settings, SDK paths, and sccache configuration                                      | Covers values Cargo and build scripts can observe at compile time                                                                                |

The Git index representation contains each tracked path, mode, stage, and blob
object ID. It is stable across checkouts while changing when a covered file is
renamed, replaced, or edited. A second SHA-256 is computed over the actual file
bytes with explicit path and size boundaries. The utility also requires covered
working-tree inputs to match the index before identity generation, cache
staging, and bundle verification.

## Asset Boundary

The reusable Rust executable is intentionally asset-free. The cache identity reads
the canonical `crates/asset-embedding-env.txt` contract, and a structural test
requires every matching V1/V2 Rust asset reference to appear in that contract.
Identity generation fails if any declared variable that embeds Bun, TUI
JavaScript, Node.js, or KAS is present. This prevents a cached executable from
carrying stale frontend or server assets.

Twinki and the TUI bundle are always built from the current checkout. Test lanes
continue to select that TUI through `KIRO_TEST_TUI_JS_PATH` or the existing E2E
harness configuration.

## Data Flow

```text
checkout + pinned tools + system dependencies
                  |
                  v
       canonical build identity
                  |
                  v
 actions/cache/restore into RUNNER_TEMP
        |                         |
  no exact hit               exact hit
        |                         |
 delete partial data      verify identity + size + SHA-256
        |                         |
execute recorded build plan          copy
        |                         |
 verify identity unchanged <-----+
        |
 stage binary + manifest
        |
 probe selected executable
        |
best-effort cache save
        |
 build current Twinki/TUI
        |
 upload per-run bundle
        |
 each lane restores mode and verifies source + OS/arch + size + SHA-256
        |
execute KIRO_CHAT_CLI_BIN
```

The identity file contains the typed build plan. The cache utility executes
that recorded plan directly, so the workflow cannot drift from the Cargo
recipe asserted by the manifest.

The cache is restored under `RUNNER_TEMP`, never directly into `target`. Only
after all checks pass is the binary copied through a temporary file into
`target/release`.

## Cache and Trust Rules

- Only `cache-hit == 'true'` is usable. Prefix or partial matches are deleted.
- Executable caches do not use `restore-keys`.
- Cross-OS archives remain disabled.
- The manifest contains the complete identity, its SHA-256, expected filename,
  byte size, and binary SHA-256.
- The cache directory must contain exactly the manifest and expected binary.
  Symlinks, extra files, path traversal, malformed manifests, and digest
  mismatches are rejected.
- Missing hosted-runner image or native-toolchain metadata disables cross-run
  reuse for that runner instead of weakening the identity.
- Cache restore/save failures do not block the normal build.
- Cargo registry `restore-keys` remain separate. That cache contains
  dependencies validated by Cargo, not an executable selected for testing.
- A cache entry is saved only after the staged executable passes its version
  probe and bundle verification.

GitHub documents that cache contents are not signed or verified. The SHA-256
manifest detects accidental corruption, not a malicious writer that can replace
both files. Writer trust comes from GitHub's cache scope:

- Scheduled runs execute the workflow on the default branch and warm trusted
  default-branch entries.
- Pull request runs may read matching default-branch entries.
- A pull request writes to `refs/pull/.../merge`; those entries are available
  only to reruns of that pull request, not `main` or other pull requests.
- Fork pull requests do not enter RC build lanes under the current resolver.

GitHub cache keys are immutable. If an exact key is occupied by invalid
contents, find its ID with
`gh cache list --repo kiro-team/kiro-cli --key rc-chat-cli-` and remove only
that entry with `gh cache delete <cache-id> --repo kiro-team/kiro-cli`.
Changing the workflow or cache utility also produces a new identity because
both are covered build inputs.

Artifact attestations are not used for this cache. GitHub recommends against
signing frequent builds used only for automated testing, and attestations add a
security benefit only when consumers verify them against a policy. The branch
scope, exact identity, and two integrity checks are the appropriate controls for
this non-release artifact.

## Failure Behavior

| Condition                                                                                                            | Result                                      |
| -------------------------------------------------------------------------------------------------------------------- | ------------------------------------------- |
| Exact cache hit and all checks pass                                                                                  | Skip Cargo and use the validated binary     |
| Cache miss, eviction, disabled cache, partial hit, or cache service error                                            | Delete staging data and build normally      |
| Exact hit with malformed manifest, wrong identity/platform/name, extra file, symlink, size mismatch, or SHA mismatch | Fail the build job                          |
| Source, toolchain, runner, command, or compile environment changes before cache staging                              | Fail instead of saving an ambiguous result  |
| Per-run artifact does not match its manifest, current source/platform, or expected Cargo recipe                      | Fail the consuming lane before executing it |

The Cargo command uses `--locked` because `Cargo.lock` is a covered input and
the post-build identity check requires it to remain unchanged. A manifest
change without the corresponding lockfile update therefore fails at the Cargo
step instead of producing a binary whose identity no longer matches its inputs.

## sccache Boundary

The pinned sccache action runs before identity generation so the wrapper path
and native tool resolution are identical during lookup, compilation, and
staging. A binary-cache hit still skips Cargo. The setup source adds the pinned
executable to `PATH` and exports `SCCACHE_PATH` plus GitHub cache-service
transport credentials. The reusable-binary identity includes the exact sccache
version, resolved wrapper path, and compile-affecting configuration, while
cache-service credentials are intentionally excluded from the persisted
manifest.

## Required CI Evidence

The implementation PR is complete only after:

1. A first RC run shows misses and successful saves for Linux, macOS, and
   Windows.
2. A rerun of the same PR shows exact validated hits and skips all three Cargo
   builds.
3. Smoke, ACP integration, and E2E lanes pass using the hit binaries.
4. Job summaries show the identity and binary SHA-256 values used by each build.

## Official References

- [GitHub dependency caching](https://docs.github.com/en/actions/reference/workflows-and-actions/dependency-caching)
- [GitHub `actions/cache`](https://github.com/actions/cache)
- [GitHub cache restore semantics](https://github.com/actions/cache/blob/main/restore/README.md)
- [GitHub cache save semantics](https://github.com/actions/cache/blob/main/save/README.md)
- [Cargo build-script change detection](https://doc.rust-lang.org/cargo/reference/build-scripts.html#change-detection)
- [GitHub-hosted runner image lifecycle](https://github.com/actions/runner-images)
- [GitHub artifact attestation guidance](https://docs.github.com/en/actions/concepts/security/artifact-attestations)
- [Pinned sccache action setup source](https://github.com/mozilla-actions/sccache-action/blob/v0.0.10/src/setup.ts)
