---
name: node22-al2-standalone-publish
description: How the Node22Al2StandalonePublish pipeline builds Node.js 22 and onnxruntime-node from source on Amazon Linux 2 and publishes them to S3 for the kiro-cli GitHub build (KAS runtime). Use when debugging that pipeline, bumping the Node or ONNX Runtime version, reproducing the AL2 native builds by hand, or hitting glibc/GLIBCXX "not found" load failures on AL2. Triggers on glibc 2.26/2.27/2.28, GLIBC_2.28 not found, libonnxruntime.so, --partly-static, AVX-VNNI/AVX-512 assembler errors, eigen hash mismatch, brazil-path -platform, or cross-platform build-ordering race.
---

# Node22Al2StandalonePublish — AL2 native builds for kiro-cli

The pipeline builds **Node.js 22** and **`onnxruntime-node`** from source on
Amazon Linux 2 (both x86_64 and arm64) and publishes the tarballs to S3, where
the kiro-cli GitHub Actions build pulls them for the KAS runtime.

- Pipeline: <https://pipelines.amazon.dev/pipelines/Node22Al2StandalonePublish> (id `9910284`)
- Owner: girpooja · email `cw-jupiter@amazon.com` · Bindle `amzn1.bindle.resource.pq77m6xp7p53mme45pha`

## The core problem this solves

KAS runs on AL2 (**glibc 2.26**), but upstream prebuilt binaries need a newer glibc:

| Artifact | Upstream needs | Symptom on AL2 |
|---|---|---|
| `node` (nodejs.org) | glibc 2.28 | `GLIBC_2.28 not found` |
| `libonnxruntime.so.1.21.0` (`onnxruntime-node` npm) | glibc 2.27 / newer C++ | `GLIBC_2.27 not found (required by libonnxruntime.so.1)` |

A native artifact's glibc floor is set by the host it is **built** on. So both
are built on the AL2 fleet, and the C++ runtime is linked **statically**
(`--partly-static` for node; `-static-libstdc++ -static-libgcc` for onnx) so
there is no `GLIBCXX`/`CXXABI` dependency either. Each native module's glibc
floor is independent of node's — building node alone is not enough; the
onnxruntime addon had to be rebuilt too.

## Packages (all under code.amazon.com/packages/…)

| Package | Role |
|---|---|
| [Node22Al2StandalonePublishCDK](https://code.amazon.com/packages/Node22Al2StandalonePublishCDK) | CDK app: the pipeline + `PublishStack` (S3 publish) |
| [NodeJS22StandaloneDist](https://code.amazon.com/packages/NodeJS22StandaloneDist) | builds the standalone `node` tarball; emits `VERSION` |
| [OnnxRuntimeNodeAl2Dist](https://code.amazon.com/packages/OnnxRuntimeNodeAl2Dist) | builds `libonnxruntime.so` + the Node binding |
| [Node22Al2StandaloneIntegTest](https://code.amazon.com/packages/Node22Al2StandaloneIntegTest) | Gamma ToD "load on AL2" integration test |
| [KiroCliDeployCDK](https://code.amazon.com/packages/KiroCliDeployCDK) | defines `KiroCliGithubActionsRole` + its S3 identity policy |
| `NodeJS` (22.x branch) | upstream source, BLT-imported; autobuild trigger only (read-only to us) |
| ~~Node22Al2PublishPipelineCDK~~ | superseded prototype; deprecated |

Pipeline flow: `Packages` → `VersionSet` (builds `AL2_x86_64` + `AL2_aarch64`) →
`PipelineUpdate` → `Packaging` (BATS) → **Gamma** (publish + ToD load-test gate) →
**Prod** (publish, manual approval).

## Config that matters (from `lib/app.ts` / README)

| Value | Setting |
|---|---|
| Pipeline account | `194704208190` (chat cli prod) |
| Gamma bucket | `kiro-cli-al2-natives-265613951504` (acct `265613951504`) |
| Prod bucket | `kiro-cli-al2-natives-194704208190` (acct `194704208190`) |
| Region | us-west-2 |
| Version set | `Node22Al2StandalonePublish/development` (builds both platforms) |
| Synth platform | `AL2_X86_64`; publishes **both** x64 and arm64 |
| GitHub reader roles | `KiroCliGithubActionsRole` in `230592382359` (gamma) / `158872659206` (prod), trust scoped to `repo:kiro-team/kiro-cli` |

## CDK: NodeJS22StandaloneDist — how node is built

`build-tools/bin/standalone-node-build`. No fork of `NodeJS`: it consumes the
GPG/checksum-verified upstream tarball vendored in `NodeJS-22.x` (`[NodeJS]pkg.src`),
applies NodeJS's own patches, and adds **one** flag: `--partly-static`. New BLT
imports on `NodeJS/22.x` flow through automatically (autobuild trigger).

- Toolchain: Brazil `CFlags`/`CFlagsGCC@10.x` (falls back to GCC 10 if `CFlags` GCC < 10).
- OpenSSL shared-linked from `NodeJSOpenssl` (`:libssl.a,:libcrypto.a`), ICU tz refresh mirrored from NodeJS.
- Build asserts no `GLIBCXX`/`CXXABI` symbols, then `sanity_test` runs the tarball under `env -i` (no Brazil env) and checks `ldd` has no `libstdc++`/`libgcc`.
- Emits `dist/VERSION` (e.g. `v22.23.1`) — the consumer reads it to find the latest release.

## CDK: OnnxRuntimeNodeAl2Dist — how onnxruntime is built

`build-tools/bin/custom-build`; options in `ort-build-options.sh` (single source
of truth, shared with the import script).

- **Pinned**, not tracked: `ORT_VERSION=1.21.0`, `ORT_COMMIT=e0b66cad…`. The version must match the app's `onnxruntime-node` dependency; the binding and library must agree. `napi_build_version=6`.
- **Offline / vendored**: `network-access = blocked`. Upstream source (`third-party-src/`), every cmake FetchContent dep (`third-party-deps/`), and `node-addon-api` headers (`third-party-node/`) are vendored. Build runs `FETCHCONTENT_FULLY_DISCONNECTED=ON` + one `-DFETCHCONTENT_SOURCE_DIR_<NAME>` per dep (list derived from the directory, not hardcoded).
- **Toolchain**: `CFlagsGCC@13.x` (GCC 13.4; ORT 1.21 needs modern C++17). Static C++ runtime via linker flags (kept visible in cmake cache/logs, not a compiler wrapper).
- **Node binding without npm**: `onnxruntime_BUILD_NODEJS=OFF` (that path runs `npm ci` + cmake-js, which needs network). Instead configures upstream's unmodified `js/node/CMakeLists.txt` directly, supplying `CMAKE_JS_INC`, `napi_build_version`, `NODE_ARCH`, `ONNXRUNTIME_BUILD_DIR`. Node-API headers come from `[NodeJS]pkg.src`, so the binding matches the Node release the pipeline builds.
- **CPU EP only** (`BUILD_FOR_NATIVE_MACHINE=OFF`); MLAS still ships AVX2/AVX-512/VNNI kernels and dispatches at runtime, so artifacts stay portable across x86_64 CPUs.
- Build asserts: no `GLIBCXX`/`CXXABI`, glibc floor ≤ 2.26, `ldd` clean, and the `.node` loads via `process.dlopen` in a real node.
- Output is a drop-in for `node_modules/onnxruntime-node/bin/napi-v3/linux/<arch>/`; `onnxruntime_binding.node` finds `libonnxruntime.so.1` via `$ORIGIN` rpath, so both files must stay side by side.
- **Updating the pin**: edit `ort-build-options.sh`, run `./import-third-party.sh` **on a networked desktop** (never in a build), commit source + deps together.

## Assembler requirements (the two distinct gotchas)

The build host's CPU need not support these ISAs — it is **assemble-time only**;
kernels are runtime-dispatched.

- **Node / simdutf** emits AVX-512 VBMI2 (`vpcompressb`) → needs **binutils ≥ 2.35**. AL2 stock `as` is 2.29 and rejects it.
- **ONNX Runtime / MLAS** int8 kernels emit AVX-VNNI (`vpdpbusds`) → needs **binutils ≥ 2.36**. 2.35 is *not* enough.
- On the Brazil fleet, `CFlagsGCC@13.x` ships binutils 2.41 and the compiler finds `as` relative to itself; `custom-build` probes exactly `$CXX -print-prog-name=as` up front so a toolchain change fails with a clear message, not a mid-build "unsupported instruction".

## PublishStack quirks (`lib/publish-stack.ts`)

- **Both arches from one x86 synth.** Resolves each dist package's per-platform farm with **`brazil-path -platform <AL2_x86_64|AL2_aarch64>`** (not `@amzn/pipelines`' platform-blind `BrazilPathArtifacts.fromPackage`). The arm build is fetched from the package master to the x86 synth host and bundled into the arm64 `BucketDeployment` — never executed there.
- **Cross-platform build-ordering race → synth polls.** The x86 synth reaching for the aarch64 build is an *undeclared* dependency; Brazil only orders declared, same-platform deps. If the arm build hasn't published, `brazil-path` returns `COMPONENT_NOT_FOUND` and the pipeline wedges. `distArtifactPath` polls (30s interval, 30 min ceiling) instead of failing. On the fleet it fails loud only after timeout; **locally it never polls** (single-platform workspace legitimately lacks the other arch — `isDeveloperLocalBuild()` = no `BRAZIL_PACKAGE_VERSION`). Race-free alternative = a second per-arch pipeline; not done, revisit if noisy.
- **Objects land at** `<prefix>/al2-<arch>/` with `.sha256` per release; `prune: false` keeps prior versions; bucket `RETAIN`, versioned, `S3_MANAGED` encryption (KMS avoided so readers don't need `kms:Decrypt`), `BUCKET_OWNER_ENFORCED`, `enforceSSL`.
- **Cross-account S3 needs both halves**: resource-side bucket policy (`s3:GetObject`/`s3:ListBucket`, here) + identity-side role policy (in `KiroCliDeployCDK`). Bucket names are explicit so the role can name their exact ARNs.
- **`node` ships `VERSION`, `onnxruntime` does not.** Both dists are in the CDK build closure; two bare `dist/VERSION` files collided with `CopyFarmConflictException`. ONNX is version-pinned and the consumer derives its version from `onnxruntime-node`'s `package.json`, so it omits the marker.

## Gamma integration test (Dogma HAVE-SOFTWARE-INTEGRATION-TEST)

`Node22Al2StandaloneIntegTest/build-tools/bin/custom-build`, run as a ToD
`TestOnDemandApprovalWorkflowStep` ("AL2 artifact load test") on the **AL2 x86_64**
shared fleet before Prod. It rebuilds nothing — it resolves the two dist tarballs
via `brazil-path [pkg]pkg.runtimefarm`, then forces the OS loader to load them:
`env -i node --version` (node ELF) and `process.dlopen(onnxruntime_binding.node)`
(loads `libonnxruntime.so`). This is exactly where a too-new-glibc build fails.
The test package is in the pipeline's autobuild `packages`, so a change to it
re-runs automatically.

## Consuming side (kiro-cli GitHub build)

`scripts/build.py` (branch `feature/kas-for-al2`) pulls per-stage via the
GitHub-OIDC-assumed reader role:

```bash
bucket=kiro-cli-al2-natives-265613951504   # gamma; prod: …-194704208190
arch=x64                                    # or arm64
# node: VERSION names the latest release, then grab that tarball
version=$(aws s3 cp "s3://$bucket/node22/al2-$arch/VERSION" -)
aws s3 cp "s3://$bucket/node22/al2-$arch/node-$version-al2-$arch.tar.xz" .
sha256sum -c "node-$version-al2-$arch.tar.xz.sha256"
# onnxruntime: version comes from the pinned onnxruntime-node package.json
aws s3 cp "s3://$bucket/onnxruntime-node/al2-$arch/onnxruntime-node-<ver>-al2-$arch.tar.xz" .
```

ONNX consume: unpack over the installed npm package **after** `npm ci` (which
fetches the incompatible upstream binaries):

```bash
tar -xJf onnxruntime-node-1.21.0-al2-x64.tar.xz --strip-components=1 \
  -C node_modules/onnxruntime-node/bin/napi-v3/linux/x64/
```

Re-apply after anything that reinstalls `onnxruntime-node` (`npm ci`, its
postinstall). Automate with a postinstall/patch step if routine.

## Reproducing the builds by hand (no Brazil, no root)

From the original investigation (`~/temp/investigations/`). Useful when
debugging outside the fleet or bringing up a new toolchain. Verified 2026-07 on
an AL2 host (glibc 2.26, stock gcc 7.3, gcc10 pkg = binutils 2.35).

**Node 22 (~1h15m on 16 cores):**
1. Build **GCC 13.4** from source (`--disable-bootstrap --disable-multilib --enable-languages=c,c++`) — stock g++ 7.3 can bootstrap it (only C++11 needed). `./contrib/download_prerequisites` first. Use the kernel.org mirror (`mirrors.kernel.org/gnu/gcc/…`); ftp.gnu.org timed out.
2. Assembler shim: symlink `gcc10-as`/`gcc10-ld` (binutils 2.35) as `as`/`ld` on PATH (AL2's 2.29 rejects `vpcompressb`).
3. `./configure --partly-static` then `make -j`. Verify: `ldd` shows no libstdc++/libgcc; `objdump -T node | grep GLIBC_ | sort -uV | tail -1` ≤ 2.26; zero `GLIBCXX|CXXABI`.

**ONNX Runtime 1.21.0 (~25 min, reuses the GCC 13 toolchain):**
1. Build **binutils 2.42** from source (2.35 is too old for `vpdpbusds`); `make MAKEINFO=true` if host lacks makeinfo. Repoint the shim `as`/`ld`.
2. CC/CXX wrapper scripts append `-static-libstdc++ -static-libgcc` (ORT has no `--partly-static`).
3. **eigen hash drift**: `cmake/deps.txt` pins the sha1 of eigen's GitLab **zip archive bytes**; GitLab regenerates archives so the hash drifts (upstream `gitlab.com/libeigen/eigen/-/issues/2744`; later ORT moved eigen to GitHub). Verify the served zip is byte-identical to a `git fetch` of the pinned **commit** (`1d8b82b0…`, content-addressed) before swapping the hash to `51982be8…`. The build's `import-third-party.sh` (`patch_eigen_hash`) does exactly this swap.
4. `./build.sh --config Release --build_shared_lib --build_nodejs --skip_tests --parallel 16 -Donnxruntime_BUILD_UNIT_TESTS=OFF`. Outputs in `js/node/bin/napi-v3/linux/x64/`.

**Verification (both):** glibc floor came out 2.18 (lib) / 2.25 (bindings/node) — well under AL2's 2.26 — with zero `GLIBCXX`/`CXXABI`. Same eigen-style hash drift can recur for any dep whose host regenerates archives; always verify against the pinned git commit before updating a hash.

## History

- Decision (2026-07-17): no NodeJS fork — consume vendored source + add `--partly-static`. `CR-290055631` closed as superseded.
- `Node22Al2PublishPipelineCDK` was the prototype; replaced by `Node22Al2StandalonePublishCDK`.

## Useful links

- [Adding stages to your pipeline](https://docs.hub.amazon.dev/pipelines/cdk-guide/howto-cdk-expand-pipeline/)
- [NativeAWS how-to guides](https://builderhub.corp.amazon.com/docs/native-aws/developer-guide/)
- [But it builds on my desktop (BRAZIL_PACKAGE_VERSION)](https://w.amazon.com/bin/view/PackageBuilder/ButItBuildsOnMyDesktop#PackageBuilder_Environment_Variables)
