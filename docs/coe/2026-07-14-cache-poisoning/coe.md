## COE: Kiro CLI 2.12.2 release blocked by stale cross-compilation build cache

**Date:** 2026-07-14
**Severity:** SEV-5 (release-blocking build failure; no customer-facing impact)
**Duration:** Release blocked 2026-07-13 18:46 UTC → ongoing (mitigation identified, not yet applied). Latent build-reliability degradation ~2026-07-02 to 07-14.
**Related Ticket:** 508977e9-5f50-4a86-9ccc-8037e38501bf ("Kiro CLI 2.12.2")

> Assertions in this document are traced to CI logs, the GitHub Actions cache API, and git history in the companion `coe-references.md`, with strength ratings. Claims that could not be directly reconstructed (intermediate caches are mutable and LRU-evicted) are called out as inferred.

---

### Summary

Kiro CLI is distributed as prebuilt binaries; the release pipeline cross-compiles the `aarch64-unknown-linux-gnu` target using `cross` (a Docker-based cross-compiler) on GitHub Actions runners. To keep the published binaries compatible with the documented "glibc >= 2.34" support floor, the cross container image is pinned by digest to Ubuntu 20.04 (glibc 2.31).

On 2026-07-13, the 2.12.2 stable build was blocked because the `aarch64-unknown-linux-gnu` job failed deterministically: cached `build-script` binaries linked against GLIBC 2.39 were restored into the build and could not execute inside the pinned 20.04 container. The 2.39 binaries had been produced legitimately weeks earlier while the upstream `cross-rs` image was Ubuntu 24.04, then persisted in the `Swatinem/rust-cache` `target/` cache and carried forward — across dependency and toolchain changes, and from `main` into the release branch — because the cache key does not include the cross-container image identity. No defective artifact was shipped: the failure is fail-safe (it blocks the build rather than producing a binary that violates the glibc floor). The release was delayed at least one day.

---

### Customer Impact

**No customer-facing impact.** The failure is a build-time error; no artifact was published (a poisoned build fails rather than producing output).

- **Duration (release availability):** 2.12.2 blocked from 2026-07-13 18:46 UTC; ongoing until the cache purge + rebuild lands (≥1 day).
- **Error / blast radius:** the `aarch64-unknown-linux-gnu` target only; other targets built successfully. Latent: ~2 weeks of intermittently failing `main` nightly builds (mixed causes; this fault a confirmed contributor, not necessarily the majority).
- **Affected customers / regions:** none.

**Near-miss:** release outcomes were nondeterministic — whether a build passed depended on cache hit/miss state, not on code. 2.12.1 (07-09) happened to hit a cache miss and shipped clean; had it hit the poisoned cache it would have failed like 2.12.2. The risk was to release predictability/availability, not to shipped-artifact correctness.

**Blast Radius Reduction:** (1) make release builds hermetic (no cache restore) so releases are deterministic; (2) an automated glibc gate on the built artifact would catch any glibc-floor regression before publish; (3) including the container image identity in the cache key prevents the poisoning entirely.

---

### Incident Analysis

#### Detection

**How was the event detected?**
Manually. The release owner observed the 2.12.2 stable build failing and escalated. No CLI-team alarm fired. The same fault had contributed to intermittent `main` nightly failures for ~2 weeks, but nightly failures were unalerted and uninvestigated.

**How could detection time be improved?**
Time-to-detect was ~11 days (fault first able to surface after the 07-02 image pin; escalated 07-13). Alerting on nightly + release build failures would have surfaced it within a day. A pre-release gating CI build exercising the cross path would catch it before a release attempt rather than during one.

#### Diagnosis & Mitigation

**How did you identify the root cause?**
Several plausible hypotheses were disproven with evidence (transient network flake, cache "leak," `cargo update` dep drift, runner OS migration, a specific code change). The decisive step compared a passing nightly against the failing release build: the nightly logged `No cache found` and compiled clean; the release logged `Cache hit ... full match: true` on an entry holding GLIBC_2.39 build-scripts. Tracing the cache key and restore-key prefix, and a 24.04-image nightly that seeded the payload, established the propagation path.

**How did you mitigate?**
Mitigation identified (not yet applied): purge the `v0-rust-build-linux-Linux-x64*` caches and re-run the stable build. A cache miss compiles clean in the pinned 20.04 container (proven by the 07-14 nightly), so this is low-risk.

**How could mitigation time be improved?**
Hermetic release builds (no cache) would remove the need for cache purges entirely and make every release build deterministic. Including the image digest in the cache key removes the poisoning class.

#### Contributing Factors

**Was this triggered by a change?**
Yes — the interaction of three CI changes over ~24h, none defective alone: PR #3335 (07-01) restructured caching and kept `target/`-level `Swatinem/rust-cache` for cross builds; PR #3337 (07-02) deduplicated dependencies, rotating the cache key; PR #3346 (07-02) pinned the cross image back to 20.04. The upstream trigger was `cross-rs` bumping its `:main` image from 20.04 to 24.04 (~Jun 25).

**Did an existing backlog item address this risk?**
No. The pre-#3335 cache step carried a comment noting that binaries "tied to the runner image break when GitHub rotates images," but the guard (`cache-bin: false`) covered only `~/.cargo/bin`, never `target/` build-script binaries. The risk class was known but not fully addressed, and no backlog item tracked it.

**When was the last ORR performed?**
None for the CI cross-compilation caching design.

---

### Timeline

All times UTC (dates from git/CI, PDT-derived).

| Time | Event |
|------|-------|
| ~Jul 1 07:56 | main nightly (job 84484739460) runs the **24.04** cross image (`63277db5…`), compiles GLIBC_2.39 build-scripts, and caches `target/` — origin of the 2.39 payload (harmless while the image is 24.04). |
| Jul 1 | PR #3335 keeps `target/`-level `Swatinem/rust-cache` for cross builds (persistence + propagation vector). |
| Jul 2 | PR #3337 dedupes deps → `Cargo.lock` change rotates the cache key; the broad restore-key prefix becomes the carry-forward path. |
| Jul 2 | PR #3346 pins the cross image back to **20.04** (`73811f43…`, glibc 2.31). Output binaries correctly re-capped at ≤ 2.34, but cached 2.39 build-scripts become unrunnable in-container. |
| Jul 2–13 | Intermittent `main` nightly failures (mixed causes); this fault confirmed on 07-11 (job 86575028077). Unalerted. |
| Jul 9 | 2.12.1 stable builds green — its aarch64-gnu job hit `No cache found` → fresh clean 2.31 build (job 86225175287). Ships cleanly; masks the live fault. |
| **Jul 13 18:46** | **START OF IMPACT** — 2.12.2 stable build (run 29275794372): attempt 1 fails on a transient rustup network error; re-runs fail on GLIBC 2.39 (poisoned cache hit) in gamma and prod. Release blocked. |
| Jul 13–14 | Investigation; hypotheses raised and disproven with evidence. |
| Jul 14 07:14 | main nightly (job 87025166148) gets `No cache found`, compiles clean 2.31, re-saves a clean cache — cache state transiently self-heals, but poison can resurface until purged. |
| Jul 14 | Root cause traced; mitigation + prevention plan defined. |
| **Pending** | **END OF IMPACT** — cache purge + 2.12.2 rebuild. |

---

### 5 Whys

#### Root Cause Analysis

**1. Why was the 2.12.2 stable build blocked?**
The `aarch64-unknown-linux-gnu` build-scripts required GLIBC_2.39, which the pinned 20.04 container (glibc 2.31) cannot provide.

**2. Why did the build-scripts require 2.39?**
They were restored from cache, not freshly compiled. (Every observed cache-miss build compiled clean 2.31 and succeeded; every observed failure was a hit on a poisoned entry.)

**3. Why was the cache poisoned with 2.39 binaries?**
They were compiled and cached while the cross image was Ubuntu 24.04 (glibc 2.39), before #3346 pinned it back — legitimate at the time.
-> **ACTION:** stop caching environment-tied executables for cross builds (move to input-keyed compilation caching).

**4. Why did those stale binaries reach the release build after the pin?**
The cache key omits the cross-image identity, and a broad restore-key prefix plus GitHub's default-branch cache fallback carried the `target/` payload forward across `Cargo.lock`/toolchain rotations and from `main` into the release branch.
-> **ACTION:** include the cross-container image digest in the cache key **and** the restore-key prefix.

**5. Why does the cache key omit the image identity, and why did release builds use the cache at all?**
Caching the cross `target/` was retained image-agnostically; the known "image-tied binaries" risk was only partially guarded (`cache-bin: false` for `~/.cargo/bin`). Separately, the guard meant to keep prod/release builds uncached checks `inputs.branch_name != 'prod'`, but `branch_name` is a git branch and never the literal `prod`, so release builds restored the cache too.
-> **ACTION:** make release builds hermetic (gate on `environment != 'prod'`); add an automated glibc guardrail.

#### Duration Analysis

**1. Why did detection take ~11 days?**
Nightly build failures were visible but unalerted and uninvestigated; there was no gating pre-release build.
-> **ACTION:** alert on nightly + release build failures; add a pre-release gating cross build.

**2. Why was the failure intermittent (hard to pin down)?**
GitHub cache LRU eviction under heavy churn (900+ caches) meant the poisoned entry flickered in and out; failures only fired on a cache hit that also forced build-script re-execution.
-> **ACTION:** cache hygiene (scope caches; avoid broad cross-branch/cross-target prefixes).

---

### Lessons Learned

- **[LL1]** Never cache environment-tied executables (build-script/proc-macro binaries in `target/`) without encoding the build environment (container image) in the cache key. glibc is an environment contract, not a source input.
- **[LL2]** Restore-key *prefixes* are a silent carry-forward vector: a poisoned payload survives lockfile/toolchain/branch changes via prefix fallback long after the primary key rotates.
- **[LL3]** Release builds should be hermetic and deterministic — a release outcome must not depend on shared, mutable CI cache state.
- **[LL4]** A guard is only as good as the variable it checks: `branch_name != 'prod'` silently disabled the intended "no cache for prod" protection.
- **[LL5]** Green builds can mask a live fault when success is cache-state-dependent; nondeterministic CI hides latent defects.
- **[LL6]** A correct guardrail (the #3346 image pin) turned a silent correctness regression into a loud, fail-safe build break — the right trade, but it needs an automated glibc gate so the check isn't manual.

---

### Action Items

Priorities: HIGH (30 days), MEDIUM (60 days), LOW (90 days). **Type** ∈ {Mitigation, Detection, Prevention}. Owners/dates TODO pending team assignment.

| Priority | Type | Action (exit criteria) | Owner | Due | Status |
|----------|------|------------------------|-------|-----|--------|
| HIGH | Mitigation | Purge all `v0-rust-build-linux-Linux-x64*` caches across refs and re-run 2.12.2 stable. Exit: 2.12.2 aarch64-gnu builds green and is promotable. | TODO | 2026-08-13 | Open |
| HIGH | Prevention | Include the cross-container image digest in the rust-cache **key and restore-key prefix** so an image change invalidates the cache and fallback cannot cross image boundaries. Exit: a forced image-digest change produces a cache miss (verified in CI). | TODO | 2026-08-13 | Open |
| HIGH | Prevention | Make prod/release builds hermetic: gate caching on `environment != 'prod'` (not `branch_name`); pin the host runner image for release builds. Exit: a release build log shows no cache restore and a pinned runner image. | TODO | 2026-08-13 | Open |
| HIGH | Prevention | Automated glibc guardrail that fails the build if any shipped glibc target's max symbol > `GLIBC_2.34`; runs on every PR/build across all glibc targets and on `Cross.toml` changes. Exit: gate merged and shown to fail on a synthetic >2.34 binary. | TODO | 2026-08-13 | Open |
| MEDIUM | Detection | Alert on nightly and release build failures. Exit: alarm/notification fires to the team channel on any failed nightly/release build; validated against the 07-11 failure. | TODO | 2026-09-12 | Open |
| MEDIUM | Detection | Add a pre-release gating CI build exercising the full `aarch64-gnu` cross path. Exit: release cannot proceed unless the gating cross build is green. | TODO | 2026-09-12 | Open |
| MEDIUM | Prevention | Produce a plan (design + prototype) to move cross builds off raw `target/` caching to input-keyed compilation caching (as native builds use in #3335). Exit: design doc reviewed and one prototype cross build validated; migration tracked as a follow-up item. | TODO | 2026-09-12 | Open |
| LOW | Prevention | Cache hygiene: scope caches to avoid broad cross-branch/cross-target restore-key prefixes; define a TTL/eviction policy. Exit: cross-cache keys are target- and scope-specific; policy documented in the CI runbook. | TODO | 2026-10-12 | Open |
| LOW | Detection | Emit the built binary's max GLIBC symbol as a nightly metric/dashboard. Exit: metric visible with ~30-day history. | TODO | 2026-10-12 | Open |
| LOW | Detection | Set `fail-fast: false` on the Linux build matrix so one target's failure does not cancel the others. Exit: matrix config updated; a single-target failure leaves siblings' results visible. | TODO | 2026-10-12 | Open |
| LOW | Prevention | Scope network retries with backoff for `rustup`/`apt`/`bun`/`awscli` fetches (do not blanket-retry whole jobs). Exit: the specific fetch steps retry with backoff; job-level retry unchanged. | TODO | 2026-10-12 | Open |

---

### Related Documents

- **Tracking ticket:** 508977e9-5f50-4a86-9ccc-8037e38501bf ("Kiro CLI 2.12.2")
- **Prior related COE / regression:** P464326498 (original glibc 2.38 regression that motivated the #3346 image pin)
- **Failing release run:** GitHub Actions run 29275794372 (jobs: gamma 86922713457, prod 86925415676)
- **Origin (24.04 image) nightly:** job 84484739460
- **Clean rebuild proof:** job 87025166148 (07-14, `No cache found`)
- **Commits:** #3335 (938b2cd4b), #3337 (945abea9b), #3346 (70a5ff19a)
- **Cross config:** `Cross.toml` (pinned image `sha256:73811f43…` = Ubuntu 20.04); 24.04 image `sha256:63277db5…`
- **Workflow:** `.github/workflows/build-linux.yml`
- **Evidence tracing:** `coe-references.md` (this directory)
