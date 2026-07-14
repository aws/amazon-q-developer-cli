# COE - Data Gathering References

Evidence gathered via the `gh` CLI (GitHub Actions logs, run/job metadata, cache API) and `git` history on `kiro-team/kiro-cli`. Every major claim in `coe-draft.md` traces to a source below, with a strength rating.

---

## Failure signature (release-blocking)

**Claim:** aarch64-gnu build fails because build-scripts require GLIBC_2.39 unrunnable in the pinned 20.04 container.

**Evidence:** failed release run 29275794372, job 86925415676 (prod aarch64-gnu):
```bash
gh run view --repo kiro-team/kiro-cli --job 86925415676 --log-failed
```
```
error: failed to run custom build command for `libc v0.2.186`
  /target/release/build/libc-*/build-script-build: /lib/x86_64-linux-gnu/libc.so.6: version `GLIBC_2.39' not found
error: failed to run custom build command for `serde_core v1.0.228`  (same)
cross build ... --target aarch64-unknown-linux-gnu --release → exit status 101
```
**Strength: STRONG.** Direct failed-step log; reproduced in both gamma (job 86922713457) and prod (86925415676) on run 29275794372.

---

## Root cause: poisoned cache, not fresh compile (miss→clean vs hit→poison)

**Claim:** the 2.39 build-scripts come from a restored cache; fresh compiles produce clean 2.31.

**Evidence — passing nightly (cache miss → clean):** job 87025166148 (07-14 main gamma aarch64-gnu):
```
Image: ubuntu-24.04 ; resolve ghcr.io/cross-rs/...@sha256:73811f43… (20.04 cross image)
No cache found.
<build succeeds>
```
**Evidence — failing release (cache hit → poison):** job 86922713457 / 86925415676:
```
Cache hit for: v0-rust-build-linux-Linux-x64-385f36d7-c0e905d7
Restored from cache key "v0-rust-build-linux-Linux-x64-385f36d7-c0e905d7" full match: true.
/target/release/build/libc-*/build-script-build: ... `GLIBC_2.39' not found
```
Correlation across observed jobs: 07-08 (miss→clean, job 85811659591), 07-11 (hit→GLIBC fail, job 86575028077), 07-13 18:59 (miss→clean, job 86909382672), 07-14 (miss→clean, job 87025166148); release 07-13 gamma+prod (hit→fail).
**Strength: STRONG.** Every observed cache-miss build succeeded; every observed failure was a cache hit. Note: "every" is over the observed sample (~6 jobs), not exhaustive — labeled inferred where generalized.

---

## The cross image is pinned to 20.04 (glibc 2.31); 24.04 = glibc 2.39

**Claim + digests.** Source: PR #3346 body (commit 70a5ff19a):
```bash
git show 70a5ff19a
```
> cross-rs bumped `aarch64-unknown-linux-gnu:main` from Ubuntu 20.04 (glibc 2.31) to 24.04 (glibc 2.39) between v2.10.0 (Jun 25) and v2.11.0 (Jul 2). Pin to the known-good 20.04 digest.
- 20.04 (pinned, good): `sha256:73811f43949215c88456a311970037a34f3f89f6486cfbcf61d8fe5379a9517e`
- 24.04 (2.39 source): `sha256:63277db516741d256d8e536a73a9fd389858c4790a9eb1a1c53fd7bbdd313853`

`Cross.toml` (current) pins `73811f43…`; failing + passing 07/13–14 jobs both resolve `73811f43…` in-log.
**Strength: STRONG.** Commit body + in-log image resolution.

---

## Origin of the 2.39 payload (24.04-window nightly)

**Claim:** a 24.04-image nightly compiled 2.39 build-scripts and cached `target/`.

**Evidence:** job 84484739460 (07-01 07:56 main gamma aarch64-gnu):
```
resolve ghcr.io/cross-rs/...@sha256:63277db5… (24.04)
No cache found.
... Saving cache ...
```
Job conclusion: success (run 28502517230).
**Strength: MODERATE / INFERRED.** Proven: it ran the 24.04 image, had `No cache found`, built successfully, and the `Saving cache` step ran. Inferred: that *this specific* job's cache is the surviving taproot, and that its save succeeded — no `Cache saved successfully` line was captured, and a 07-02 sibling (job 84665554505) logged `Failed to save … another job may be creating this cache`. The origin is best described as a *class* of late-June/early-July 24.04-image nightlies.

---

## Carry-forward vector: broad restore-key prefix

**Claim:** the payload survives lockfile/toolchain/branch changes via the hash-less prefix `v0-rust-build-linux-Linux-x64`.

**Evidence:** restore-key list logged by the cross jobs, e.g. job 84665554505:
```
v0-rust-build-linux-Linux-x64-488a6f2f-6b759801   (primary; note toolchain hash 488a6f2f)
v0-rust-build-linux-Linux-x64-488a6f2f            (restore-key)
- v0-rust-build-linux-Linux-x64                    (broad restore-key, no hash)
```
Later jobs use toolchain hash `385f36d7` and lockfile hash `c0e905d7` — different from `488a6f2f`, yet the broad prefix still matches.
**Strength: STRONG for the mechanism (restore-keys present in-log); INFERRED for the exact propagation chain** (intermediate caches are mutable/evicted and unrecoverable).

---

## Cache key omits image identity + the ineffective prod guard

**Claim:** cache keyed on host+toolchain+Cargo.lock, not image; and `branch_name != 'prod'` never disables caching for release builds.

**Evidence:** `.github/workflows/build-linux.yml`:
```yaml
- name: Cache Rust dependencies (cross builds)
  if: inputs.branch_name != 'prod' && (matrix.musl || matrix.cross_target)
  uses: Swatinem/rust-cache@v2
  with:
    cache-bin: "false"
```
`branch_name` derivation in `.github/workflows/build-and-release.yml` meta step: nightly→`main`, stable→`release/X.Y.Z`, feature→`feature/…` — never literal `prod`. The prod-environment job on run 29275794372 (job 86925415676) restored the cache (`full match: true`), confirming caching was active for a "prod" build.
**Strength: STRONG.** Workflow source + observed prod-job cache restore.

---

## Contributing CI changes (by PR)

```bash
git log --since='2026-06-23' -- .github/workflows/build-linux.yml Cross.toml scripts/
```
- **#3335 (938b2cd4b, 07-01)** "replace rust-cache with sccache for cross-branch compilation caching": native → sccache; **cross builds keep `Swatinem/rust-cache`**. Removed a prior comment: *"Don't cache `~/.cargo/bin`: rustup-managed binaries are tied to the runner image and break when GitHub rotates images."* (guard covered only `~/.cargo/bin`).
- **#3337 (945abea9b, 07-02)** "registry cache fix + dependency deduplication": dedupes deps (strum, rand, kiro-bot ACP, winnow) → changes `Cargo.lock` (key rotation).
- **#3346 (70a5ff19a, 07-02)** pins the cross image to 20.04.
**Strength: STRONG.** Commit diffs + messages. Attribution note (blameless): the three PRs above are the only glibc/cross/caching commits in the window; an earlier hypothesis pointing at an unrelated change was checked and disproven. Individually reasonable changes; the fault is their interaction, not any one author's error.

---

## Cargo.lock last changed 07-08 (why the current key is recent)

```bash
git log --since='2026-07-01' -- Cargo.lock   # 07-08 (#3390), 07-06, 07-02, 06-29 …
git show origin/release/2.12.2:Cargo.lock | grep -A1 'name = "libc"'   # 0.2.186
```
Failing versions `libc 0.2.186` / `serde_core 1.0.228` are already pinned in the committed lock on both `main` and `release/2.12.2` (identical), disproving "cargo update dep drift."
**Strength: STRONG.**

---

## Poisoned cache entry metadata

```bash
gh api "repos/kiro-team/kiro-cli/actions/caches?key=v0-rust-build-linux-Linux-x64-385f36d7-c0e905d7"
```
```
id=5726564153  ref=refs/heads/main  size≈539 MiB  created=2026-07-14T08:02:05Z  last_accessed=2026-07-14T08:02:05Z
```
Current entry was (re)written by the 07-14 successful nightly (clean). Repo holds 916 total caches (heavy LRU pressure).
**Strength: STRONG.** Cache API. Note: this proves the *current* entry is clean; the poisoned instance the 07-13 release restored is gone (overwritten/evicted), so "self-heal" is transient until purge.

---

## 2.12.1 shipped on a cache miss (nondeterminism / near-miss)

**Evidence:** 2.12.1 run 29048615526; aarch64-gnu gamma job 86225175287:
```
resolve ghcr.io/cross-rs/...@sha256:73811f43… (20.04) ; No cache found ; <success>
```
2.12.1 run overall failure was Windows smoke tests, not Linux.
**Strength: STRONG.**

---

## Detection & timeline anchors

- Transient attempt-1 failure: run 29275794372, `curl: (7) Couldn't connect to server` fetching `sh.rustup.rs` (dtolnay/rust-toolchain "install rustup" step). **STRONG.**
- ~2-week nightly instability with mixed causes: run list `gh run list --workflow build-and-release.yml`; 07-11 (86575028077) confirmed this fault; 07-13 07:37 (29232696417) failed on Windows smoke tests (different cause). **STRONG for the two cited; the split of causes across all failures is not fully enumerated (inferred).**

---

## Not verified / open

- The specific run that saved the poisoned `…-c0e905d7` cache the 07-13 release restored (intermediate caches evicted).
- Unique count of affected nightly runs attributable to this fault vs other causes.
- Whether host-runner image identity also needs to be in the cache key (build-scripts are container-linked, so the cross-image digest is the load-bearing key; host image is likely not required but unverified).
