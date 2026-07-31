# macOS Bun JIT input-lag investigation

Date: 2026-07-30

Status: root cause reproduced; signing fix implemented

Machine-readable evidence:
[`../perf-baselines/macos-bun-jit-causality-2026-07-30.json`](../perf-baselines/macos-bun-jit-causality-2026-07-30.json)

## Executive summary

The macOS release build of the V2 TUI was much slower than running the same TUI
from source with Bun. Long responses and inputs around 4,000 words made
keystrokes visibly lag and arrive in bursts.

The release pipeline downloads the pinned upstream Bun binary, then signs and
notarizes it in `scripts/build.py::sign_bun_per_arch`. That signing call did not
provide an entitlement file. Re-signing replaced Bun's upstream signature and
removed its JIT entitlements.

Under the macOS hardened runtime, JavaScriptCore cannot use its normal JIT code
generation path without `com.apple.security.cs.allow-jit`. Bun still runs and
produces correct output, but CPU-heavy JavaScript executes much more slowly.
This made the issue look like a Twinki input or buffer problem even though the
runtime signature was the controlling difference.

A same-source-Bun experiment measured:

| Metric | Hardened, no JIT | Hardened, `allow-jit` | Improvement |
| --- | ---: | ---: | ---: |
| JIT-sensitive integer loop | 1699.37 ms | 69.95 ms | 24.29x |
| Twinki frame | 116.43 ms | 10.00 ms | 11.65x |
| Key to terminal repaint | 156.92 ms | 22.63 ms | 6.94x |
| Key to observed input state | 253.49 ms | 27.19 ms | 9.32x |

The frame-time reduction was 91.4%. The repaint-latency reduction was 85.6%.

## Runtime path

The development and release paths use equivalent TUI code but not equivalent
Bun signatures:

1. `bun ./src/index.tsx` uses the locally installed upstream Bun runtime.
2. The Darwin release job downloads Bun in `sign_bun_per_arch`.
3. CD Signer replaces Bun's upstream signature.
4. The signed per-architecture Bun is uploaded and later embedded in
   `kiro-cli-chat`.
5. The shipped CLI extracts and runs those re-signed bytes.

The adjacent Node signing path already passes `node-entitlements.plist`. The
Bun path called `sign_and_notarize` without `entitlements_path`, so the generic
signing support existed but was not used.

This fix belongs in `kiro-team/kiro-cli`. The autocomplete repository packages
the already-built `kiro-cli-chat`; it does not independently download or sign
the embedded Bun runtime.

## Controlled experiment

The causal verifier copied one upstream Bun 1.3.13 file twice. Both copies were
ad-hoc signed with the hardened-runtime option:

- Control: no entitlements.
- Treatment: only `com.apple.security.cs.allow-jit`.

The response was captured from the prompt
`Explain Rust in detail with all details you possibly can`. Replaying that
fixed payload removed model and network timing from the measurement.

The verifier then:

1. Confirmed the control lacked `allow-jit` and the treatment contained it.
2. Ran the same integer loop three times in each runtime.
3. Checked that all arithmetic results were identical.
4. Launched the same minified Kiro TUI bundle against mock ACP.
5. Replayed the same 3,993-word, 1,118-line response.
6. Sent the same 20 keystrokes.
7. Checked the final input, canary text, process exit code, bundle hash, and
   payload hash.
8. Failed unless the microbenchmark ratio was at least 10x and the TUI frame
   ratio was at least 5x.

Controlled input fingerprints:

| Input | SHA-256 |
| --- | --- |
| Source Bun before the two signatures | `fc0b4cae13a911098f0c61d13b7d9fd6b640bdb9f6b6a0b78bdb9d778c12bc3f` |
| Minified TUI bundle | `577403a6efb9014d80ab4eb32c173e6ce8807c85de6dd6a2848d5ed794d93c97` |
| Replayed response | `53a03025a178aaa1346c9efb2109715aab0f34120adf0d28aa9923e632d4c3ef` |

The signatures necessarily changed the final executable hashes. The
pre-resigning source file, Bun version and revision, JavaScriptCore revision,
bundle, response, terminal dimensions, and keystrokes were held constant.

An independent rerun on 2026-07-31 passed the same thresholds:

| Metric | Hardened, no JIT | Hardened, `allow-jit` | Improvement |
| --- | ---: | ---: | ---: |
| JIT-sensitive integer loop | 1712.60 ms | 66.11 ms | 25.91x |
| Twinki frame | 111.83 ms | 10.06 ms | 11.12x |
| Key to terminal repaint | 141.24 ms | 23.09 ms | 6.12x |

The exact five-key release profile was also applied to a third copy of the same
Bun file. The integer loop averaged 65.96 ms versus 1718.65 ms with no
entitlements, a 26.05x improvement with identical arithmetic results.

## Additional controls

The wider investigation also compared:

| Comparison | Frame average | Repaint average |
| --- | ---: | ---: |
| Live source response | 9.44 ms | 18.24 ms |
| Live installed binary response | 85.28 ms | 64.93 ms |
| Fresh bundle, upstream Bun 1.3.13 | 9.54 ms | 22.00 ms |
| Fresh bundle, extracted release Bun 1.3.13 | 112.71 ms | 161.74 ms |
| Ad-hoc hardened signature, no entitlement | 112.24 ms | 184.91 ms |
| Ad-hoc hardened signature, only `allow-jit` | 10.21 ms | 25.50 ms |

The live source response was slightly larger than the installed-binary
response: 3,267 versus 3,075 words. Response size therefore did not explain the
source advantage.

The controls ruled out:

- Bun 1.3.5 versus 1.3.13.
- TypeScript source versus a minified production bundle.
- The Toolbox wrapper versus direct execution.
- Model response timing and KAS/network behavior.
- The lite-rollout environment flag.

Paired CPU profiles used the same minified bundle and response. The no-JIT run
took 10.9 seconds and collected 11,201 samples; the JIT run took 3.7 seconds
and collected 1,826 samples. The same hot minified function had 3,663 ms versus
393 ms of self time. The no-JIT profile also spent substantially more time in
markdown regular expressions, character scanning, Yoga, and render
orchestration. This was broad JavaScript slowdown, not one pathological
Twinki buffer operation.

In a separate live observation, peak CPU fell from about 80% with the shipped
no-JIT runtime to about 20% with JIT enabled: roughly 4x lower, or a 75%
reduction. This was an Activity Monitor-style peak observation rather than a
harness-controlled metric, so it is directional evidence and is not used as a
regression threshold.

## Why large responses expose it

An input update performs React reconciliation, markdown and text processing,
Yoga layout, Twinki frame composition, terminal diffing, and output. With JIT,
the captured long-response state rendered in about 10 ms. Without JIT, it took
about 116 ms.

A 116 ms frame consumes about seven 60 Hz frame budgets. Keystrokes arriving
during that work wait behind the render loop, which explains the delayed and
bursty input observed with long scrollback and large pasted prompts.

## Entitlement decision

Bun's official
[macOS code-signing documentation](https://bun.com/docs/bundler/executables#code-signing-on-macos)
says to include an entitlement plist with JIT permissions and publishes this
five-key profile:

- `com.apple.security.cs.allow-jit`
- `com.apple.security.cs.allow-unsigned-executable-memory`
- `com.apple.security.cs.disable-executable-page-protection`
- `com.apple.security.cs.allow-dyld-environment-variables`
- `com.apple.security.cs.disable-library-validation`

The pinned upstream Bun 1.3.13 arm64 signature contains the same five keys.
The extracted Amazon-signed Bun used in the failing comparison contained none.

Only `allow-jit` was needed to restore the measured performance. The release
fix nevertheless preserves Bun's complete documented profile to retain
upstream runtime, FFI, native-library, and DYLD compatibility.

These are security-sensitive hardened-runtime exceptions:

| Entitlement | Capability |
| --- | --- |
| `allow-jit` | Allows the process to create JIT executable memory using the supported macOS mechanism |
| `allow-unsigned-executable-memory` | Allows broader use of unsigned executable memory |
| `disable-executable-page-protection` | Relaxes executable-memory page protection |
| `allow-dyld-environment-variables` | Allows DYLD environment variables under hardened runtime |
| `disable-library-validation` | Allows loading code not signed by Apple or the same team |

The broad profile is an intentional upstream-compatibility decision, not a
claim that all five keys improve TUI performance. A least-privilege alternative
would ship only `allow-jit`, but that requires separate Bun compatibility
validation and deliberately diverges from Bun's published signing guidance.

## Fix and regression gate

The Darwin Bun signing path now:

1. Supplies `bun-entitlements.plist` to `sign_and_notarize` for both x64 and
   arm64.
2. Verifies the returned code signature.
3. Uses `codesign --display --xml --entitlements <temporary plist>` and
   `plistlib` to parse the signed entitlement dictionary without depending on
   stdout/stderr routing.
4. Fails if any requested key or value is absent.
5. Continues to notarization and final artifact upload only after that check.

The assertion is in the shared signing helper, so the existing Node
entitlement path also fails closed if CD Signer drops requested capabilities.

## Memory caveat

The synthetic replay reported 126.61 MiB RSS without JIT and 361.05 MiB with
JIT, a 2.85x increase in that one end-of-run sample. Heap and external-memory
accounting also differed, so the sample may reflect JIT code, GC timing, and
different progress through queued work rather than steady-state production
memory.

This is not evidence that the release will always add 234 MiB, but it is a real
tradeoff signal. Release-candidate validation should record idle RSS, peak RSS,
and RSS after a fixed long-response workload before rollout.

## Release validation

The complete release path should verify:

1. Both signed Bun architecture artifacts contain all five expected keys.
2. Notarization succeeds.
3. `kiro-cli-chat` is built from those exact artifacts.
4. The autocomplete package retains the entitlements after installation and
   first-run extraction.
5. A long-response replay remains near the JIT-enabled baseline.
6. Idle, peak, and post-workload RSS remain acceptable.
