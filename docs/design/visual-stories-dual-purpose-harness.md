# Visual Stories Dual-Purpose Harness

Status: proposed implementation plan

## Objective

Use one set of TUI stories to provide both:

1. deterministic CI certification, including semantic interaction checks and
   cell-level visual regression; and
2. a self-contained HTML story explorer, with an optional local live mode for
   editing supported properties and regenerating terminal frames.

The design also lets Knight Rider publish and inspect the same frame format
without merging its exploratory process runner into the Visual Stories runner.

## Delivery Priority

P0 is the verification system: one resolver, deterministic terminal-frame
capture, semantic checks, complete-manifest validation, candidate evidence,
reviewable diffs, and safe baseline enforcement in RC Certification.

`bun run stories:serve` is the next consumer. The P0 contracts include explicit
scalar controls and a reusable explorer model now, but the live server and
dynamic property editing do not block P0. They must reuse the P0 resolver and
capture path rather than creating another renderer.

## Current State

The Visual Stories lane currently:

- runs every registered story and variant in an isolated PTY;
- retains focused semantic assertions and `play()` journeys for complex
  surfaces;
- binds journey state coverage to the exact named capture and its assertions;
- derives positive coverage from captures that completed successfully;
- keeps planned catalog analysis separate from captured runtime evidence;
- preserves successful state evidence when a later journey capture fails;
- classifies integration-only, product, and visual-baseline gaps without
  counting them as covered;
- stores one terminal-cell frame and derives text and HTML from it;
- writes text, rendered HTML, a manifest, coverage reports, and an aggregate
  report; and
- runs on Linux, macOS, and Windows in RC Certification.

The current capture is useful evidence, but it is not yet a visual-regression
contract:

- baseline differences are reported but do not fail the lane;
- the complete capture plan is not resolved before execution;
- profiles, provenance fingerprints, and baseline promotion are not yet
  enforced; and
- Knight Rider has a separate frame and report representation.

Knight Rider already supplies a live PTY, WebSocket streaming, keyboard and
resize endpoints, named frame capture, and an offline report. It should become
a producer of the shared evidence model, not a second story runner.

## Proven Reference Lessons

A separate prototype with hundreds of deterministic terminal frames provides
useful evidence about which mechanisms survive at scale. This design adopts the
mechanisms, not that prototype's monolithic capture script:

- rendered terminal cells remain stable when equivalent raw ANSI streams do
  not;
- a small generated critical-frame projection gives fast local feedback while
  the complete catalog remains the coverage and release authority;
- deterministic fixture capture and live model-driven evidence require
  different comparison policies;
- a failed recapture writes candidate evidence and never overwrites the last
  approved baseline;
- canonical output rejects filtered, limited, or partial captures;
- source and runtime fingerprints are checked before and after a long capture;
- deterministic timestamps and canonical frame ordering make regenerated
  reports reproducible;
- sanitized ANSI, text, and frame sidecars materially improve diagnosis; and
- terminal output must be treated as potentially sensitive before upload.

The prototype artifact is tens of megabytes at full scale. The implementation
therefore measures checked-in bytes, uploaded bytes, browser parse memory, and
first-render time before deciding whether diagnostic sidecars ship in every
successful CI artifact or only on failure.

## Design Principles

1. **Stories are the source of fixtures.** CI, the offline explorer, and local
   live rendering consume the same story definitions.
2. **Workflow Monitor is the scaffold.** Its typed fixtures, semantic
   assertions, `play()` interactions, and named captures become the reusable
   pattern for other complex surfaces.
3. **Terminal cells are the visual truth.** Raw ANSI, generated HTML, and pixels
   are representations, not the canonical assertion surface.
4. **Semantics and visuals are complementary.** Text and interaction assertions
   explain expected behavior; cell-grid comparison catches unexpected layout,
   color, and styling changes.
5. **Adapters stay independent.** Visual Stories controls deterministic
   component fixtures. Knight Rider controls arbitrary live processes.
6. **Offline evidence is inert.** The artifact browses captured data and never
   evaluates repository code or starts a process.
7. **Live controls are explicit.** Only declared, JSON-serializable properties
   can be changed through the browser.
8. **CI fails closed.** Missing, extra, malformed, or changed baseline frames
   fail verification. Baseline updates require a separate explicit command.
9. **Cross-platform output is one product contract.** Linux, macOS, and Windows
   compare against the same canonical frame unless a reviewed normalization is
   justified by terminal protocol behavior.
10. **One corpus has multiple evidence tiers.** A fast critical projection,
    complete deterministic certification, and exploratory live evidence differ
    in execution and comparison policy, not story semantics or frame format.
11. **Canonical publication is all or nothing.** Focused and partial runs are
    useful diagnostics but can never become release authority.

## Architecture

```text
                         STORY DEFINITIONS
              fixtures + variants + visual contract
                                 |
                 +---------------+---------------+
                 |                               |
                 v                               v
          static variant                    play() journey
          render + capture             input + wait + named states
                 |                               |
                 +---------------+---------------+
                                 |
                                 v
                     Visual Stories adapter
                   resolve + validate + execute
                                 |
                                 v
                      PTY + xterm cell buffer
                                 |
                                 v
                    TerminalFrameSource.flush()
                    captureVisibleFrame()
                                 |
                         TerminalFrame v1
                                 |
             +-------------------+-------------------+
             |                   |                   |
             v                   v                   v
       semantic result     baseline comparison   evidence explorer
                           structured cell diff


       Knight Rider adapter
       arbitrary process + input
                  |
                  v
       PTY + xterm cell buffer
                  |
                  +--------> the same TerminalFrame v1 and HTML renderer
```

The dependency direction is deliberate and has two shared layers:

```text
terminal-frame                         visual-evidence
capture + compare + primitive HTML     manifest + diff report + explorer
          ^                                      ^
          |                                      |
          +--------------+-----------------------+
                         |
             +-----------+-----------+
             |                       |
      Visual Stories            Knight Rider
             ^
             |
      story resolver
             ^
             |
      story definitions
```

`terminal-frame` knows nothing about manifests, stories, React components,
Knight Rider, HTTP routes, or CI. `visual-evidence` accepts open producer
metadata and does not import either adapter. Neither adapter imports the other.

## Story Scaffolding

The first deliverable generalizes what Workflow Monitor already does well:

```text
Story fixture
   |
   +-- static variant ----------> render + frame
   |
   +-- semantic contract -------> ready condition + meaningful invariants
   |
   +-- play() journey ----------> input + wait + named capture assertions
```

Use three levels deliberately:

| Story kind                   | Required contract                                                                                   |
| ---------------------------- | --------------------------------------------------------------------------------------------------- |
| Small presentational variant | It resolves, renders, stays alive, and produces a visual frame.                                     |
| Stateful component           | The above plus assertions for meaningful state, not decorative wording.                             |
| Complex surface              | Typed fixture builder, semantic invariants, `play()` journey, and assertions on each named capture. |

Do not manufacture low-value string assertions for every catalog variant. A
semantic assertion earns its maintenance cost only when it explains behavior
that a cell diff cannot: selected ownership, permission placement, navigation,
state transition, preserved output, or exit behavior.

One resolver owns story behavior. It must:

- merge supported meta-level and story-level args;
- resolve the final render component and props;
- normalize current `parameters.certification` compatibility input;
- validate visual controls and property overrides;
- produce stable story, variant, profile, and capture IDs; and
- reject unsupported CSF features with a clear error.

The current implementation resembles CSF but does not implement full CSF
semantics: it drops meta `args` and `argTypes`. The plan therefore calls this the
local story format until the resolver tests prove the supported CSF subset.

## Shared Terminal Frame Contract

Add two test-support modules. They initially live beside the current PTY owner;
if the PTY harness is extracted into a package, they move with it rather than
creating a reverse dependency from production TUI code.

```text
terminal-frame/
  types.ts
  capture.ts
  normalize.ts
  compare.ts
  serialize.ts
  render-html.ts

visual-evidence/
  manifest.ts
  explorer.ts
```

`TerminalFrame` is versioned independently from the evidence manifest:

```ts
interface TerminalFrameV1 {
  schemaVersion: 1;
  viewport: {
    columns: number;
    rows: number;
  };
  buffer: 'normal' | 'alternate';
  cursor: {
    column: number;
    row: number;
    visible: boolean;
  };
  palette: TerminalPaletteV1;
  styles: readonly TerminalStyleV1[];
  rows: readonly (readonly TerminalCellV1[])[];
}

interface TerminalCellV1 {
  text: string;
  width: 0 | 1 | 2;
  styleIndex: number;
}

interface SerializedTerminalRunV1 {
  styleIndex: number;
  cells: readonly SerializedTerminalCellTokenV1[];
}

type SerializedTerminalCellTokenV1 =
  | readonly [text: string, width: 0 | 1 | 2]
  | readonly [repeat: number, text: string, width: 0 | 1 | 2];

interface TerminalStyleV1 {
  foreground: TerminalColorV1;
  background: TerminalColorV1;
  bold: boolean;
  dim: boolean;
  italic: boolean;
  underline: boolean;
  blink: boolean;
  strikethrough: boolean;
  overline: boolean;
  inverse: boolean;
  invisible: boolean;
}

type TerminalColorV1 =
  | { mode: 'default' }
  | { mode: 'palette'; index: number }
  | { mode: 'rgb'; value: number };

interface TerminalPaletteV1 {
  defaultForeground: number;
  defaultBackground: number;
  ansi: readonly number[]; // Exactly 256 RGB integers.
}
```

Every row expands to exactly `viewport.columns` cells. The frame contains
exactly `viewport.rows` rows and never includes scrollback. Width-zero
continuation cells are retained so wide and combining characters compare
correctly. The logical frame and serialized baseline encoding are versioned
separately. Logical frames use explicit cells and a frame-local style table;
serialization groups adjacent styles and uses repeat tokens for blank or
identical cells. Comparison never depends on compression choices.

The first implementation records the public xterm `IBufferCell` attributes:
foreground/background color mode and value, width, characters, bold, dim,
italic, underline, blink, inverse, invisible, strikethrough, and overline.
Underline style and underline color are deliberately excluded because the
pinned public API does not expose them. Cursor visibility is tracked from split
and reset terminal mode sequences because the buffer API exposes cursor
position but not visibility. RGB values are integers from `0x000000` through
`0xffffff`; `ansi` resolves all 256 palette entries. Adding another terminal
attribute requires a frame schema version bump or a backward-compatible
optional field with a canonical default.

Serialization orders styles by their canonical JSON value, rows from top to
bottom, and cells from left to right. Profile resolution supplies the default
foreground/background and 256-entry palette, so HTML rendering never guesses a
terminal color.

`PtyManager` owns the private xterm instance and exposes a narrow source:

```ts
interface TerminalFrameSource {
  captureVisibleFrame(options: {
    quietMs: number;
    timeoutMs: number;
  }): Promise<TerminalFrameV1>;
}

serializeTerminalFrame(frame): string
parseTerminalFrame(json): TerminalFrameV1
compareTerminalFrames(expected, actual): TerminalFrameDiff
renderTerminalFrameHtml(frame): string
```

Every PTY data write increments an internal revision.
`captureVisibleFrame()` alone owns the safe sequence: await all queued xterm
parser callbacks, wait for a quiet revision window, flush again, then
synchronously read the cells. Producers cannot bypass or reorder those steps,
and the xterm instance and revision stay private.

HTML is always generated from `TerminalFrame`. No producer stores independently
captured HTML as visual truth.

## Evidence Model

The shared manifest wraps frames with producer-specific metadata:

```ts
type EvidenceManifestV1 = EvidenceManifestBaseV1 &
  (
    | {
        mode: 'planned';
        plan: {
          digest: string;
          expectedFrameCount: number;
        };
      }
    | {
        mode: 'exploratory-session';
        plan?: never;
      }
  );

interface EvidenceManifestBaseV1 {
  schemaVersion: 1;
  producer: string;
  suite: string;
  generatedAt: string;
  complete: boolean;
  actualFrameCount: number;
  source: {
    commit: string;
    platform: 'linux' | 'macos' | 'windows';
    terminal: 'xterm-headless';
    terminalVersion: string;
    unicodeVersion: string;
    normalizationVersion: string;
    sourceDirty: boolean;
    sourceFingerprint: string;
    runtimeFingerprint: string;
    externalInputsFingerprint: string;
  };
  frames: readonly EvidenceFrameV1[];
  extensions?: Readonly<Record<string, JSONValue>>;
}

interface EvidenceFrameV1 {
  id: string;
  label: string;
  sequence: number;
  facets: Readonly<Record<string, string>>;
  verification:
    | { mode: 'exact-frame'; authority: 'certification' | 'informational' }
    | { mode: 'checks-only'; authority: 'informational'; reason: string };
  attachments: {
    frame: EvidenceAttachmentV1;
    text?: EvidenceAttachmentV1;
    ansi?: EvidenceAttachmentV1;
  };
  checks: readonly EvidenceCheckV1[];
  comparisonResult?: FrameComparisonResult;
  extensions?: Readonly<Record<string, JSONValue>>;
}

interface EvidenceAttachmentV1 {
  kind: 'terminal-frame' | 'text' | 'ansi';
  path: string;
  mediaType: string;
  sha256: string;
  bytes: number;
  sanitized: true;
}

interface EvidenceCheckV1 {
  id: string;
  kind: string;
  status: 'passed' | 'failed';
  message?: string;
}
```

Producer metadata remains in the open extension object. Shared rendering and
comparison code consumes only the common fields.

The evidence writer derives every attachment path as a normalized relative
POSIX path from a cryptographic digest of the frame ID and attachment kind.
Producers never provide artifact paths. The staged writer rejects absolute
paths, `..`, backslashes, duplicate normalized paths, path escapes after
real-path resolution, and every symlink. It verifies attachment byte counts,
hashes, media types, kinds, and sanitation markers before publication. All
reads, validation, and uploads stay under one newly created run-owned root.

Frame IDs are stable and independent of execution order:

```text
<story-id>--<variant-id>--<profile-id>--<capture-id>
```

Duplicate IDs fail before evidence is written. Visual Stories supplies
`story`, `variant`, `profile`, and `capture` facets plus its typed extension.
Knight Rider supplies session and process facets without pretending to be a
story.

Before execution, the resolver writes a sorted capture plan containing every
story, variant, profile, and required capture known before `play()` runs. A
journey declares complete capture definitions in metadata and may not invent
IDs at runtime. The Storybook adapter owns this compiler; it is not part of
`visual-evidence`. The plan digest covers structural IDs, suite configuration,
frame schema, normalization policy/version, and xterm version. It excludes
source commit, generated timestamps, execution order, and display labels so an
unchanged plan compares across commits.

Every shard carries the same plan digest. A per-platform shard merge rejects
mixed source commits, dirty-state identities, source fingerprints, runtime
fingerprints, external-input fingerprints, platforms, suites, profile
definitions, normalization versions, frame schemas, plan digests, duplicate
IDs, missing IDs, extra IDs, and incomplete shards. Source commit and
provenance are validated separately from the plan digest. The RC collector
then requires one complete final manifest for each expected platform. A final
manifest sets
`complete: true` only after those checks pass. The supervisor supplies one run
start timestamp to every shard; merge never derives `generatedAt` from shard
completion order.

Knight Rider is exploratory and has no predeclared capture plan. Its manifest
uses `mode: 'exploratory-session'` with no `plan`.
`complete: true` means the session closed and all captured frames were
serialized; it does not imply a predetermined count.

Every planned Visual Stories frame requires
`{ mode: 'exact-frame', authority: 'certification' }`. Exploratory manifests
are always informational; they may request exact comparison for a deterministic
shell state, but cannot become baseline authority. Nondeterministic live content
uses `checks-only` with a non-empty reason.

The RC collector treats a malformed manifest, `complete: false`, count
mismatch, or missing expected platform artifact as a lane failure rather than a
partial success.

Manifest result counts are derived from validated frame records. Writers do not
provide an independently trusted summary, and the RC collector recomputes every
aggregate before publishing it.

### Evidence Tiers

All tiers use `TerminalFrameV1` and the same resolver:

| Tier             | Inventory                                                     | Comparison                                                                      | Authority                              |
| ---------------- | ------------------------------------------------------------- | ------------------------------------------------------------------------------- | -------------------------------------- |
| Critical         | A reviewed, generated projection of high-risk frame IDs       | Exact deterministic cells                                                       | Fast local and unit-test feedback only |
| Certification    | The complete planned story, variant, profile, and capture set | Exact deterministic cells                                                       | Blocking RC visual contract            |
| Exploratory live | Captures chosen by an interactive or model-driven producer    | Structure and declared checks; exact content only when explicitly deterministic | Informational evidence                 |

The critical projection is generated from the approved certification bundle by
the baseline update command. It records the source bundle digest and selected
frame IDs; a test rejects a stale projection. Engineers never hand-edit two
independent expected frames.

Live model output is not exact-hashed merely to create a green metric. Its
manifest still validates schema, frame integrity, secret scanning, producer
checks, and any deterministic shell states. Content-excluded frames cannot
contribute to deterministic certification counts.

## Story Contract

Keep the currently supported `args`, `render`, and `play` concepts. Add an
explicit `visual` contract while accepting the existing
`parameters.certification` shape during migration:

```ts
interface LocalStoryMeta {
  id: string;
  // Existing component, args, argTypes, parameters, and title remain.
}

interface LocalStoryVariant {
  id: string;
  // Existing args, render, parameters, and play remain.
}

interface StorybookVisualParameters {
  suites?: readonly string[];
  viewport?: StorybookViewport;
  ready?: {
    text: string;
    timeoutMs?: number;
  };
  settle?: {
    quietMs?: number;
    timeoutMs?: number;
  };
  invariants?: StorybookAssertions;
  editableProps?: readonly string[];
  profiles?: readonly string[];
  captures?: Readonly<Record<string, StoryCaptureDefinition>>;
}

type StoryControlValue = string | number | boolean;

type ResolvedStoryControl =
  | { type: 'boolean' }
  | { type: 'text'; maxLength?: number }
  | { type: 'number'; min?: number; max?: number; step?: number }
  | { type: 'select'; options: readonly StoryControlValue[] };

interface StoryCaptureDefinition {
  label: string;
  assertions?: StorybookAssertions;
  coversVisualStates?: readonly string[];
}

interface StorybookPlayContext {
  // Existing input and wait methods remain.
  capture(id: string): Promise<void>;
}
```

Rules:

- Story modules and variants declare explicit stable IDs independently from
  titles, component names, export names, labels, and source paths.
- During migration, the resolver records `idSource: 'explicit' |
'legacy-derived'` and derives legacy IDs with the current title/export slug
  algorithm. Baseline promotion rejects any catalog plan containing a
  legacy-derived ID.
- Renaming a title or export while keeping its explicit ID changes display or
  source metadata only. Changing an explicit ID is an intentional remove/add
  baseline diff.
- Capture definitions are the single owner of IDs, labels, and state-specific
  assertions. `play()` only refers to an existing ID.
- Journey coverage names the visual states proved by each capture. Variant-level
  state coverage is reserved for single-frame stories.
- Capture assertions apply to that named state.
- Visual invariants apply to every frame because they describe properties that
  must remain true throughout the journey.
- Explicit capture assertions do not leak into another state.
- A variant with no `play()` gets one automatic `default` capture.
- A variant with `play()` gets only its declared explicit captures; the runner
  does not silently add a final frame.
- Story, variant, profile, and capture IDs match
  `[a-z0-9]+(?:-[a-z0-9]+)*`, which forbids the `--` frame-ID delimiter.
  Labels are separate display text.
- Complex values, functions, React nodes, and store fixtures are not editable.
- Undeclared properties are rejected by the live API.
- Existing `certification.suite`, `readyText`, `settleMs`, and assertions are
  adapted in one resolver. Existing variant assertions become journey
  invariants, while capture assertions and state ownership remain local to the
  named frame.
- Meta args, story args, controls, and overrides are resolved in one pure,
  tested function before React renders.
- Existing `tags` become catalog facets. The resolver supports the scalar
  `argTypes.control` subset for boolean, text, finite number, and scalar select
  options. A prop is live-editable only when named in `editableProps`; naming a
  missing, complex, or unsupported `argType` fails resolution.
- Existing complex `argTypes` may remain documentation-only. Unsupported
  execution semantics such as loaders or decorators fail explicitly instead
  of being silently ignored.

The resolver normalizes the supported authoring fields into
`ResolvedStoryControl`. Live requests contain
`Record<string, StoryControlValue>`, validate finite numbers, text length,
select membership, and viewport bounds, and reject nested JSON.

### Render Profiles

Avoid an unbounded Cartesian product. Define reviewed named profiles:

```ts
interface VisualProfile {
  id: string;
  theme: 'dark' | 'light';
  colorDepth: 'truecolor' | '256' | '16';
  display: {
    animations: 'frozen';
    ascii: boolean;
    icons: boolean;
  };
  viewport?: StorybookViewport;
}
```

The resolver produces one `ResolvedRenderEnvironment` containing the profile,
sanitized process environment, provider values, viewport, terminal name,
locale, clock, and random seed. `run-storybook.tsx` consumes that object rather
than independently inferring settings from the host.

The default catalog profile is `dark-truecolor`. A small required compatibility
set covers light theme, 256 colors, ASCII/no-icons, and narrow width. Stories
opt into additional profiles by ID; suite configuration can require a profile
for selected component categories without editing every story. Sharding must
land before profile multiplication is enabled in CI.

Profiles use the actual application seams:

- `KIRO_TERMINAL_THEME` or an explicit `ThemeProvider` theme;
- `KIRO_TUI_FORCE_COLOR`, not the unrelated `FORCE_COLOR`;
- a `GlyphsProvider` initialized from profile display settings;
- an animation clock/provider that can be frozen; and
- isolated settings files for any values still read at module load.

## Deterministic Capture

Each Visual Stories render receives an isolated `KIRO_HOME` and a fully pinned
environment:

- explicit locale and timezone;
- explicit terminal type, color depth, theme, and viewport;
- animations disabled or advanced by a test clock;
- stable clock and random seed where content exposes time or randomness;
- no inherited user configuration; and
- an allowlisted child environment rather than a merge with all host variables.

Before the first PTY starts, the supervisor records:

- the checked-out source revision and dirty state;
- a source fingerprint covering story definitions, fixtures, renderer code,
  frame/evidence code, lockfiles, and suite configuration while excluding
  generated evidence and baseline output;
- a runtime fingerprint covering Bun, xterm, Twinki, and platform identities;
  and
- any external producer input required by the suite.

It recomputes the same values after the last PTY exits and before publishing.
Any change fails the run and leaves only non-authoritative diagnostic evidence.
The source and runtime fingerprints are evidence provenance; they remain
separate from the structural capture-plan digest.

Capture waits for both:

1. the declared ready condition; and
2. xterm parser flush followed by a quiet buffer-revision window.

`settleMs` remains a compatibility fallback, not the primary readiness signal.
After final flush, the runner rechecks readiness and assertions against the
active viewport so text that appeared and disappeared cannot produce a stale
pass.

Execution is supervised outside each story process:

- ready, `play()`, and each input/wait operation have bounded deadlines;
- each variant has a default 30-second deadline with a suite-configured,
  reviewed override;
- the catalog supervisor has a 17-minute deadline, below the RC job's
  capture-step timeout; and
- the supervisor writes an append-only frame journal and an initial
  `complete: false` manifest before launching variants.

Each completed frame is flushed to the journal. On variant or suite timeout the
supervisor kills the child, records the last safely captured frame and process
state, terminates the complete process tree, finalizes an incomplete failed
manifest, and exits before the capture-step hard kill. The artifact upload
therefore has evidence even for a hung journey.

The workflow gives capture an 18-minute step timeout and the Visual Stories job
a 30-minute timeout. This reserves time outside capture for checkout,
dependency setup, fail-closed collection, and `if: always()` artifact upload.
The job timeout must remain at least the measured p99 setup time plus the
capture-step timeout plus a five-minute collection/upload reserve; changing any
budget requires updating all three values and their invariant test. If the
supervisor itself is killed, the already-written incomplete manifest remains
the evidence.

Termination uses a tested process-tree abstraction on every platform, followed
by a bounded wait for all descendants. Infrastructure retries use a fresh
attempt directory and fresh isolated home; an attempt cannot read or append to
another attempt's journal, manifest, PTY, or temporary files.

A convergence sample and an infrastructure retry attempt are different
identities. Each required sample has a stable `<platform, sample-index>` key and
may contain multiple attempt directories. The pinned collector input selects
exactly one successful immutable result for each required sample. Failed retry
attempts remain diagnostic attachments, cannot count toward convergence, and
do not become unexpected manifest inputs. A sample with no successful attempt
fails attestation.

Network isolation is not claimed unless CI enforces it. Stories must not require
network access, and a later network guard can make that invariant enforceable.

The current RC workflow retries the entire visual command for any nonzero exit.
Baseline enforcement replaces that with two steps:

1. capture may retry only a small allowlist of PTY allocation or process-spawn
   infrastructure errors and retains every attempt; and
2. semantic and baseline verification runs once over the completed manifest.

Story exit, stale readiness, timeout, assertion failure, missing frame, and
visual difference are non-retryable. The generic outer retry wrapper is removed
from the Visual Stories lane when this split lands.

### Artifact Sanitation

Normalization, comparison, and persistence execute in this order:

1. parser-flushed cells and optional raw ANSI exist only in memory;
2. terminal-aware scanning inspects raw control payloads, visible cells, text
   projections, labels, errors, machine paths, and ephemeral ports;
3. a sensitive or host-specific deterministic frame receives a failing check
   and is ineligible for baseline comparison or promotion;
4. a clean frame receives only the versioned terminal-protocol normalization
   recorded in its plan and bundle, then exact comparison runs;
5. the canonical clean frame is serialized; and
6. every diagnostic and metadata value is sanitized, rescanned, hashed, and
   written through the contained attachment writer.

Normalization never removes secrets, paths, ports, timestamps, or product text
to make frames equal. Those values indicate a fixture or product leak to fix.
Redaction is only a persistence safety mechanism for failed diagnostics.

Sanitation fails closed when a sensitive value crosses unsupported terminal
control semantics and cannot be redacted without changing interpretation.
Redaction uses fixed-cell-width markers so diagnostic layout remains useful,
but a redacted diagnostic can never become a certification frame.
Fixtures use obvious non-secret placeholders, and tests cover bearer/basic
tokens, credential assignments, URL user info, access-key shapes, OSC/DCS
payloads, wide cells, and split control sequences.

## Foundation Rule

PRs may split implementation, but they may not introduce temporary public
contracts. The first merged slice must contain:

- the one story resolver and supported local story-format contract;
- stable story, variant, profile, and capture IDs;
- state-specific semantic assertions;
- the `TerminalFrameSource` parser-flush boundary;
- versioned terminal frame and evidence schemas; and
- compatibility adapters for existing stories.

Later PRs add producers, profiles, and UI over those contracts. They do not
replace a Workflow-only API, reinterpret IDs, or migrate to a second manifest.

## Baseline and Diff Workflow

Store canonical style-table and repeat-token encoded bundles under:

```text
packages/tui/visual_tests/baselines/
  storybook-catalog/
    current.json
    bundles/
    critical.json
```

`storybook-catalog` is the only baseline authority. `critical.json` is a
generated projection of frames from its current bundle. Workflow Monitor,
Crew Monitor, category, and single-story commands are focused executions of the
same corpus; they do not own independent golden sets.

Commands:

```bash
# Local comparison without modifying baselines.
bun run test:storybook:visual -- --suite storybook-catalog --verify

# Generate a reviewed baseline proposal and its diff report.
bun run test:storybook:visual -- --suite storybook-catalog --update

# CI workers use the base-SHA harness against the candidate source tree.
bun "$TRUSTED_VISUAL_CLI" capture \
  --candidate-root "$CANDIDATE_ROOT" \
  --suite storybook-catalog \
  --out "$ATTEMPT_ROOT"

# The base-SHA collector consumes the complete pinned manifest inventory.
bun run test:storybook:visual:attest -- \
  --inputs "$PINNED_INPUT_MANIFEST" \
  --base-baseline "$BASELINE_ROOT" \
  --proposal "$PROPOSED_BASELINE_ROOT" \
  --out "$ATTESTATION_ROOT"
```

For the first baseline only, `--bootstrap-empty` replaces `--base-baseline`.
The flags are mutually exclusive, and bootstrap verifies the pointer is absent
from the pinned base checkout rather than trusting candidate inputs.

`--verify` and `--update` are mutually exclusive. Verify fails for:

- a changed viewport, cell character, width, style, color, or cursor;
- a missing expected frame;
- a new frame without a baseline;
- duplicate frame IDs;
- an unsupported schema version; or
- a semantic assertion failure.

The diff identifies text-only, style-only, cursor, viewport, missing, and extra
changes. The HTML report renders expected, actual, and a highlighted cell diff.
CI uploads the report even when verification fails.

### Baseline Authority and Promotion

The baseline at the PR base SHA is the authority for that run. Candidate source
may modify baseline files and verification code, but those changes are a
proposal, not an authority that can validate itself.

Both branch and fork RC runs use this split:

```text
candidate SHA                         trusted base SHA
-------------                         ----------------
TUI components                        story resolver + plan compiler
stories + fixtures      imports       trusted render host + PTY capture
proposed baseline      ---------->     checks + sanitation + serialization
                                       base baseline + comparator
                                       proposal/convergence validator
                                        |
                                        v
                              attested RC result + diff
```

The trusted visual CLI is checked out from the pinned base SHA. It owns story
resolution, capture-plan compilation, profile resolution, the render-host shell,
PTY lifecycle, xterm frame capture, semantic checks, sanitation, serialization,
manifest writing, baseline comparison, and attestation. The render host imports
candidate TUI components, story definitions, and fixtures from the explicit
candidate root; it does not invoke the candidate capture adapter or candidate
verification scripts.

The collector consumes artifacts produced by that trusted CLI and pinned to the
candidate SHA, platform, plan digest, source/runtime fingerprints, and capture
attempt. Candidate scripts cannot replace the resolver, runner shell, capture
path, checks, collector, or base baseline input.

The collector recomputes the candidate source fingerprint from the checked-out
candidate tree and derives runtime identity from trusted workflow inputs; it
does not trust fingerprint strings supplied by the candidate manifest. Artifact
names and digests are matched to the workflow run and exact candidate SHA
before any frame is accepted.

If the PR does not change baseline files, any deterministic difference from the
base baseline fails. If it proposes a baseline change, the trusted collector
also requires:

1. the only baseline edits are a complete immutable bundle, generated
   `critical.json`, and the corresponding pointer;
2. all nine convergence captures match the proposed normalized bundle;
3. every semantic, security, completeness, and provenance check passes; and
4. the proposed diff from the base baseline is published for human review.

The local `--update` command creates this proposal from one complete local
capture; it does not confer CI authority. `--capture` emits one immutable,
attempt-specific candidate manifest. An ordinary verification supplies one
complete sample per expected OS. A baseline proposal supplies exactly three
successful samples per OS; infrastructure retries remain subordinate attempts
within a sample. The workflow records the expected sample set and selected
successful attempt digests in a pinned input manifest. The base-SHA
`visual:attest` command rejects missing, extra, duplicate, or unpinned samples;
in proposal mode it recomputes the normalized bundle from all nine and verifies
byte equality with the proposal. It emits a context-bound attestation
containing the workflow run, base SHA, candidate SHA, selected input artifact
digests, plan digest, normalization version, proposal digest, and result.

CI never pushes a pointer or bundle. The proposal becomes authority only after
the attestation passes, human review accepts the published diff, and merge
places it on the target branch.

The trusted collector supports the current stable evidence schema. A schema
change uses a two-step migration: first teach the base collector to accept and
validate both versions, then switch candidate producers in a later PR. The
candidate can never require unreviewed validator code to interpret its own
evidence.

The base collector maintains an explicit trusted-surface path set covering the
visual CLI, resolver/compiler, render host, PTY/frame/evidence core, assertion
engine, sanitation, baseline policy, and RC orchestration. A PR that changes any
trusted surface may exercise the old base harness but cannot also propose a
baseline pointer or bundle change. The harness change merges first; a later PR
can generate a proposal after that implementation is part of the base SHA.

Initial bootstrap is permitted only when the trusted collector already exists
at the pinned base SHA and that base genuinely has no `current.json` for the
suite. The collector then treats the authority as an empty frame set, requires
the same nine-run convergence and all other promotion checks, and publishes
every proposed frame as an addition. The PR introducing the collector cannot
also bootstrap a baseline because the collector is not trusted until it merges.
Candidate deletion or replacement of a base pointer cannot re-enter bootstrap
mode; once the target branch has a pointer, only normal proposal validation is
allowed.

Running `--update` twice from an unchanged tree must produce no Git diff.
Timestamps, absolute paths, process IDs, platform path separators, and execution
duration never enter baseline files.

Every non-matching complete verification writes a candidate evidence bundle and
diff report under the run output, outside the checked-in baseline path. The
approved `current.json` pointer is unchanged. A successful verification removes
only stale generated run candidates; it never deletes immutable approved
bundles.

The local proposal writer refuses update unless its run is strict, unfiltered,
unlimited, uses every required profile and local shard, matches the complete
capture plan, has stable before/after provenance, and contains no failed
semantic or security check. The trusted attestation additionally requires all
nine platform samples. `--story`, tag/category filters, frame limits,
exploratory manifests, and incomplete platform sets must target a separate
output directory and cannot acquire canonical authority.

Baseline update uses immutable content-addressed bundles:

1. record a source-content digest and dirty state, excluding baseline and
   evidence output paths;
2. in explicit update mode, write a complete immutable bundle under
   `bundles/<bundle-digest>.json`;
3. validate its schema, plan digest, normalization policy/version, expected
   frame set, and semantic results;
4. atomically replace the small `current.json` pointer using a three-platform
   tested file-replacement helper; and
5. remove unreferenced update bundles only after pointer replacement.

An interruption leaves either the old complete bundle or the new complete
bundle active, never a partial active baseline. An orphan immutable update
bundle is safe and can be cleaned on the next run.

Before any baseline proposal is accepted, run the complete capture three
independent times on each CI platform. The gate requires:

- identical intra-platform frame hashes for all three runs;
- identical normalized hashes across all nine runs; and
- zero unclassified differences.

The published cross-platform diff inventory classifies each initial difference
as:

- a product inconsistency to fix;
- a missing fixture/environment pin to fix;
- a terminal-protocol difference with a narrow normalization and red test; or
- a blocker to single-baseline enforcement.

Cross-platform CI then compares each platform to the same baseline. Every later
baseline proposal repeats the nine-run convergence gate; a normalization,
runtime, frame-schema, or profile change cannot reuse an older attestation. Do
not create platform baselines merely to make a mismatch pass. If a single
baseline is not proven stable, visual evidence remains informational and Slice
3 cannot be marked complete.

The prior directory-replacement design is intentionally rejected because
replacing a non-empty directory is not an atomic cross-platform operation.

## Offline HTML Explorer

Generate one self-contained `index.html` from the manifest and frame files. It
provides:

- category, component, variant, profile, and capture navigation;
- search and status filters;
- expected, actual, and diff views;
- semantic assertion results;
- viewport and profile metadata;
- keyboard navigation;
- frame replay for multi-capture journeys; and
- links to coverage details.

The explorer embeds its CSS, JavaScript, manifest, and frame data. It has no CDN
or network dependency and includes a restrictive Content Security Policy.
All story text, labels, and frame data are escaped before insertion.

The uploaded artifact embeds compressed canonical frame data in `index.html`,
so the explorer remains functional as one file. It may additionally emit
sanitized `.frame.json` and `.txt` projections plus associated raw-ANSI
sidecars for machine inspection and failure diagnosis. Sidecars are never
visual truth and are not required to browse the explorer. Per-frame HTML is
unnecessary because it would duplicate the shared renderer.

If measured budgets cannot support one embedded explorer, changing to an
offline bundle is a reviewed contract change rather than quietly retaining the
"self-contained HTML" claim. Inline scripts use a generated CSP nonce or hash,
and serialized JSON escapes `</script>`.

Property controls are shown read-only in offline artifacts with a clear
message: live regeneration requires the local story server. The artifact never
claims it can execute a control.

## Local Live Story Mode

This is a post-P0 consumer of the verification foundation. P0 defines and tests
the resolver, controls, frame schema, and explorer data model so live serving
does not introduce a second contract, but P0 does not need a server to block
visual regressions.

Add a localhost-only server that reuses the explorer shell. The initial property
editing protocol is intentionally request/response:

```text
GET  /api/catalog
POST /api/renders
GET  /api/renders/:id
```

`POST /api/renders` accepts:

```ts
interface RenderStoryRequest {
  clientId: string;
  revision: number;
  storyId: string;
  variantId: string;
  profileId: string;
  overrides: Readonly<Record<string, StoryControlValue>>;
  viewport?: StorybookViewport;
}
```

The server validates every override against the story control schema, writes a
temporary render request, and restarts the isolated story PTY. Newer revisions
cancel older processes for the same client, and responses carry the revision so
the browser ignores stale results. Restarting gives each property change clean
React and store state. Persistent rerendering and terminal streaming require a
separate ordered protocol with backpressure and are not implicit extensions of
this API.

The response uses the same `TerminalFrame` model as CI. Users can save
exploratory frames, but live captures do not modify baselines. The explicit CLI
`--update` path remains the only baseline-proposal writer.

Shared live-server middleware used by both story live mode and Knight Rider:

- binds to `127.0.0.1`;
- requires a per-process token for HTTP and WebSocket requests;
- validates `Host` and `Origin`;
- requires JSON content types on mutation routes;
- enforces body, value, and request-rate limits; and
- never evaluates browser-provided code.

HTTP clients use `Authorization: Bearer <token>`. Browser WebSockets used by
Knight Rider first obtain a short-lived, single-use ticket over authenticated
HTTP and present that ticket during upgrade; non-browser clients may send the
bearer header directly. Tokens and tickets never appear in generated evidence.

## Knight Rider Integration

Knight Rider keeps:

- arbitrary command launching;
- live input and process lifecycle;
- workspace and KAS options; and
- LLM-oriented wait and control endpoints.

It replaces its private `Frame` and report generator with:

- `TerminalFrameSource.captureVisibleFrame()`;
- `EvidenceManifestV1`;
- `renderTerminalFrameHtml()`; and
- the shared explorer generator.

Knight Rider also keeps a separate transcript artifact containing scrollback.
The viewport-only `TerminalFrame` does not silently replace its current
full-buffer evidence.

The story server does not gain arbitrary command execution. Knight Rider does
not gain story fixtures or baseline-update authority.

## First Semantic Expansion: Crew Monitor

Refactor `CrewMonitor.stories.tsx` around typed fixture builders and add focused
states:

1. multiple agents with stable selection;
2. running and completed tool activity;
3. failed agent with preserved output;
4. pending dependency topology;
5. permission request routed to the selected agent;
6. keyboard navigation between agents; and
7. exit from the monitor back to chat.

Each state declares semantic assertions. Navigation, permission placement, and
exit behavior use `play()` with named captures. The story must remove the
current `any` casts in touched fixture setup rather than expanding them.

This is the proving slice for state-specific assertions and journey replay. It
does not wait for every catalog component to gain bespoke semantic checks.

## CI Rollout

These are reviewable implementation slices, not temporary architectures. Slice
1 establishes the final dependency direction and public contracts.

### Slice 1: Durable story and frame foundation

- Add the one story resolver and supported local story-format tests.
- Add explicit story and variant IDs, preserve current legacy derivation only
  as migration input, and migrate the complete registered catalog before
  baseline promotion.
- Migrate Workflow Monitor through the resolver without changing its behavior
  or frame set.
- Add state-specific capture assertions and stable IDs.
- Add parser flush, revisions, frame capture, serialization, comparison, and
  primitive HTML rendering.
- Add capture-plan and complete-manifest validation.
- Make Visual Stories emit frames from the shared core.
- Add source/runtime fingerprinting, canonical-output policy, and the one
  artifact-sanitation boundary.
- Prototype at least two compact frame encodings against the current complete
  catalog and select one using checked-in size, artifact size, parse memory, and
  comparison speed.
- Commit explicit budgets in
  `packages/tui/visual_tests/visual-budgets.json` before freezing
  `TerminalFrameV1`.

### Slice 2: Reuse proof and determinism

- Add state-specific assertions.
- Add the Crew Monitor fixture and journey set.
- Keep workflow monitor semantic coverage unchanged.
- Run three independent captures on Linux, macOS, and Windows and resolve the
  complete cross-platform diff inventory.
- Add stable sharding and plan-digest merge validation before enabling more
  profiles.

### Slice 3A: Trusted verifier and explorer

- Add transactional `--verify` and `--update`, candidate evidence, and strict
  refusal to promote focused or partial captures.
- Replace the generic visual-lane retry with capture/verify separation.
- Land the trusted visual CLI, collector, render host, path policy, and
  attestation command without a baseline or blocking gate.
- Publish actual cells, semantic checks, coverage, and diagnostics in the
  searchable explorer and RC summary.

### Slice 3B: Baseline bootstrap

- With Slice 3A now present at the base SHA, generate nine converged captures
  and check in the first deterministic catalog baseline.
- Generate the fast critical projection from the approved complete bundle.
- Validate proposed baseline changes against nine candidate-SHA captures and
  the explicit empty bootstrap authority.
- Reject a baseline proposal that changes any trusted visual-harness surface.
- Publish the complete addition diff and require a passing bootstrap
  attestation before merge.

### Slice 3C: Blocking RC gate

- With both collector and baseline now present at the base SHA, enable
  authoritative verification for branch and trusted post-approval fork RC.
- The base harness owns plan compilation, render hosting, PTY capture, checks,
  and evidence; candidate source remains read-only and cannot invoke update
  mode or upload repository changes.
- Publish expected, actual, cell diff, semantic checks, and coverage in the
  searchable explorer and RC summary.

P0 completes only after Slice 3C is merged and the required RC check exercises
the trusted collector and non-empty base baseline.

### Slice 4: Verification profile expansion

- Add approved dark/light, color-depth, accessibility, and viewport profiles to
  deterministic certification without an unbounded Cartesian product.
- Re-run convergence and extend the approved baseline for the added profiles.

### Slice 5: Live properties

- Add `bun run stories:serve` as the secured localhost live-story server with
  revisioned dynamic recapture.
- Activate the already-defined controls and profiles in the live server.

### Slice 6: Knight Rider adapter

- Move Knight Rider to the shared frame and evidence contract.
- Preserve its existing routes through compatibility wrappers.
- Preserve scrollback as a separate transcript artifact.
- Use the shared explorer and live-server security middleware.

### Slice 7: Scale by evidence

- Adjust shard count using measured runtime.
- Increase required semantic journeys by risk, not by arbitrary string count.

Slices may be separate PRs, but no slice introduces a Workflow-only API, a
second manifest, unstable identifiers, or a control format that a later slice
must replace.

## Machine-Checkable Acceptance Criteria

### Frame core

- A fixture containing default, palette, and RGB colors; foreground and
  background; bold, dim, italic, underline, blink, strikethrough, overline,
  inverse, invisible, wide, and combining cells round-trips without loss.
- Two different ANSI streams that produce the same xterm cell grid compare
  equal.
- One changed character, width, color, style, cursor, or viewport produces the
  corresponding structured diff category.
- A frame always has exactly the visible viewport and excludes scrollback.
- Palette defaults, active-buffer type, cursor visibility, xterm version, and
  Unicode version survive capture and evidence serialization.
- HTML generated from a parsed frame matches HTML generated before
  serialization.
- Cursor hide, show, reset, and split escape sequences produce the expected
  visibility state.

### Story adapter

- Existing workflow monitor variants and journeys retain their semantic
  assertions and frame counts.
- Assertions attached to one capture do not run against another capture.
- `play()` cannot supply a label, assertion, or undeclared capture ID at
  runtime.
- Duplicate story, variant, profile, or capture IDs fail before report
  generation.
- A plan with any legacy-derived story or variant ID can run diagnostically but
  cannot update or verify a canonical baseline.
- Title, export, label, and source-path changes preserve frame IDs when explicit
  IDs are unchanged.
- A crashed story process produces a failed frame with its exit code.
- Runtime output exactly matches the generated capture plan; the test does not
  hardcode a catalog count.
- Meta args, story args, and validated overrides resolve to one deterministic
  final props object.
- Existing tags and documentation-only complex `argTypes` remain valid;
  editable scalar `argTypes` resolve to controls; unsupported execution fields
  fail with the field and story ID.

### Baselines and CI

- Changed, missing, and extra frames each make `--verify` exit nonzero.
- `--verify` cannot write under the baseline directory.
- A failed verification writes candidate evidence and preserves the approved
  pointer and bundle byte-for-byte.
- Filtered, limited, focused, incomplete, exploratory, or profile-reduced runs
  cannot update or promote the canonical baseline.
- An interrupted or failed `--update` leaves either the prior or the new
  complete bundle active and never points at a partial bundle.
- Two consecutive `--update` runs from an unchanged tree produce no diff.
- The critical projection contains only approved complete-bundle frames and
  fails if its recorded source bundle digest is stale.
- Candidate changes to the baseline or verifier cannot replace the base-SHA
  resolver, render host, capture path, checks, collector, comparator, or
  authoritative baseline.
- A PR touching a trusted visual-harness surface and baseline proposal is
  rejected with the changed trusted paths.
- A proposed baseline is accepted only when the trusted collector proves it
  equals all nine complete candidate-SHA convergence captures.
- Initial bootstrap succeeds only when the base SHA has no suite pointer and
  still passes all nine convergence captures.
- Planned frames require exact certification comparison; exploratory frames are
  informational and cannot contribute to certification counts or promotion.
- Three independent runs on each OS have identical intra-platform hashes and
  identical normalized hashes across all nine runs before one shared baseline
  becomes blocking.
- Visual mismatches are not retried; infrastructure failures retain attempt
  evidence.
- Convergence counts successful samples, not retry attempts; each required
  platform/sample key selects one immutable success or fails attestation.
- RC summary links the uploaded explorer and reports changed-frame counts.
- Incomplete manifests, count mismatches, mixed plan digests, mixed source
  commits, dirty states, source/runtime/external-input fingerprints,
  normalization versions, duplicate frames, missing shards, and unexpected
  shards each fail aggregation.
- Fork RC execution verifies the same plan and baseline without gaining
  baseline-write authority.
- Frame artifacts use writer-derived relative paths, contain no symlinks, and
  cannot escape or alias another file in the run-owned root.

### Explorer and live mode

- The offline explorer works with network disabled.
- Search and each story/variant/profile/status filter have browser-level tests.
- Journey replay follows manifest sequence rather than filename order.
- Offline controls cannot issue render requests.
- Live rendering accepts each declared scalar control and rejects unknown,
  nested, non-finite, wrong-type, oversized, out-of-range, and invalid-select
  overrides.
- A newer client revision cancels or supersedes an older render, and a stale
  result cannot replace the current browser frame.
- Live rendering cannot write or update a baseline.
- Both live servers listen only on loopback, require their request token, and
  reject invalid Host, Origin, content type, body size, and rate.

### Crew Monitor

- All seven planned states produce named frames with semantic checks.
- Navigation proves selection and detail content move together.
- A permission request appears under the correct agent and nowhere else.
- Exit returns to the chat surface without leaving monitor-only text visible.

### Operational limits

- `visual-budgets.json` records maximum checked-in baseline bytes, uploaded
  artifact bytes, peak explorer parse memory, explorer first-render time, and
  per-platform lane time using Slice 1 measurements.
- CI fails when a committed budget is exceeded; raising one requires a reviewed
  diff with new measurements.
- A merged manifest is deterministic regardless of shard completion order.
- The Visual Stories job timeout reserves the measured p99 setup duration and
  five minutes for fail-closed collection/upload outside the capture timeout.
- Source and runtime fingerprints are identical before and after capture; a
  changed fingerprint prevents canonical publication.
- Manifest aggregate counts are recomputed from frames and reject a forged or
  stale summary.
- No uploadable artifact contains credential-shaped fixture values, and a
  deterministic secret leak remains a failing frame after redaction.
- Typed attachment descriptors match contained files by kind, media type, byte
  count, digest, sanitation state, and writer-derived path.

### Required red tests

- Xterm parsing is delayed past a quiet PTY period; capture still waits for the
  parser callback.
- A producer cannot access a synchronous unsafe frame-capture method.
- Host theme, color, locale, settings, and `NO_COLOR` variables are poisoned;
  the resolved render environment remains unchanged.
- Ready text appears and then disappears before quiescence; stale readiness
  does not pass.
- A story exits unexpectedly after rendering text; the frame fails.
- A journey and the suite supervisor each exceed their deadline; both preserve
  a journal and finalize an incomplete failed manifest before CI's hard timeout.
- A timed-out process leaves no descendant alive, and a retry cannot read the
  prior attempt's output.
- A semantic or visual mismatch uses the non-retryable verification path.
- Baseline update is interrupted between candidate write and replacement.
- Candidate code changes its capture adapter, verifier, and baseline to emit an
  incorrect base-matching frame; the base-SHA capture path ignores those
  scripts and still observes the candidate renderer.
- A PR changes a trusted capture surface and proposes a baseline in the same
  diff.
- Candidate source deletes `current.json`; bootstrap remains disabled because
  the base SHA contains the pointer.
- A baseline proposal differs from one of the nine convergence captures.
- A filtered capture attempts to write or promote canonical output.
- Source, runtime, or external producer input changes during capture.
- A token is split across text and terminal control payloads.
- A machine path or port changes between captures; sanitation cannot normalize
  the frames into equality.
- Frame metadata supplies an absolute path, traversal, backslash alias,
  duplicate normalized path, or symlink.
- A shard is missing, duplicated, from another source commit, or from another
  plan digest, dirty state, source/runtime fingerprint, external-input
  fingerprint, or normalization version.
- A fork run attempts baseline update and is rejected.
- Shards complete in different orders but merge to byte-identical manifests.

## Risks and Controls

| Risk                                        | Control                                                                                   |
| ------------------------------------------- | ----------------------------------------------------------------------------------------- |
| Animation or timer flakiness                | Freeze animations and inject stable time before capture.                                  |
| Parser race after PTY output                | Await xterm parser callbacks and quiet buffer revisions.                                  |
| False diffs from HTML serialization         | Compare canonical cells; derive HTML afterward.                                           |
| Hidden scrollback changes                   | Capture exactly the active visible viewport.                                              |
| Lost Knight Rider history                   | Preserve scrollback as a separate transcript artifact.                                    |
| Cross-platform PTY differences              | One baseline plus narrow, tested normalization.                                           |
| Baseline churn                              | Stable IDs, deterministic serialization, explicit update mode.                            |
| Artifact growth                             | Style-table encoding, repeat tokens, measured budgets, sharding before profile expansion. |
| Partial evidence reported as green          | Capture-plan digest, complete flag, expected counts, strict aggregation.                  |
| Partial capture replaces authority          | Canonical-output policy rejects filters, limits, missing profiles, and incomplete shards. |
| Failed recapture destroys approved evidence | Candidate output is separate and the immutable approved pointer is unchanged.             |
| Stale source captured during a long run     | Source/runtime fingerprints are checked before and after capture.                         |
| Secrets leak through diagnostic evidence    | Terminal-aware fixed-width sanitation plus a failing security check.                      |
| Candidate approves its own baseline         | A base-SHA collector validates pinned candidate artifacts and every proposal.             |
| Job timeout prevents evidence upload        | Job timeout reserves measured setup and five post-capture minutes.                        |
| Artifact path escapes the run root          | Writer-derived paths, normalization checks, real-path containment, and no symlinks.       |
| Labels or export names churn baselines      | Explicit story and variant IDs are required before promotion.                             |
| Shards mix source or runtime state          | Per-platform merge requires identical complete provenance identities.                     |
| Redaction hides nondeterminism              | Scan first, fail leaked values, compare only clean versioned normalization.               |
| Live evidence inflates certification        | Typed comparison authority keeps every exploratory frame informational.                   |
| Low-value semantic assertions               | Require them for behavior and journeys, not every decorative variant.                     |
| Browser-driven code execution               | Explicit scalar controls, schema validation, loopback token, no `eval`.                   |
| Story format silently diverges from CSF     | One tested resolver and explicit rejection of unsupported fields.                         |
| One oversized runner                        | Shared data core with independent producer adapters.                                      |
| Retry hiding a regression                   | Separate retryable capture setup from one-shot verification.                              |

## Maintenance Invariants

The architecture is acceptable only if these common changes stay local:

| Change                                    | Expected files                                                  |
| ----------------------------------------- | --------------------------------------------------------------- |
| Add a simple visual variant               | Its story file and existing registry entry only.                |
| Add a semantic state to a complex surface | Its typed fixture/story file only.                              |
| Add a journey capture                     | Its story file; no runner or report edit.                       |
| Add an editable scalar property           | Its `argTypes` and `editableProps` declaration; no server edit. |
| Add a render profile                      | Profile registry plus story/suite opt-in; no capture-core edit. |
| Add an evidence producer                  | A new adapter; no terminal-frame schema edit.                   |
| Change a display label                    | No baseline ID or capture-plan churn.                           |

If a future change violates this table, it is evidence of missing resolver
capability or producer leakage into a shared layer, not a reason to add another
special case.

## Decisions That Must Not Drift

- HTML is a review surface, never the baseline source.
- Visual snapshots do not replace semantic workflow and Crew Monitor checks.
- The offline report never executes stories.
- Live property editing never accepts arbitrary JavaScript or serialized React
  values.
- Story and Knight Rider process control remain separate adapters.
- CI and local live mode consume the same story resolver and frame serializer.
- Platform-specific baselines require a new reviewed design decision.
- Focused captures and exploratory live evidence never become canonical
  certification.
- Raw ANSI and diagnostic sidecars never participate in baseline equality.

## Audit Record

### Round 1

Architecture/extensibility and reliability/CI auditors independently rejected
the first draft as implementation-ready. All substantive findings were
accepted:

1. split producer-neutral terminal frames from open visual-evidence metadata;
2. put parser flush, revision, and capture behind the PTY-owned source;
3. add one tested story resolver rather than assuming unsupported CSF behavior;
4. preserve Knight Rider scrollback separately;
5. include palette and terminal rendering context;
6. make capture IDs and assertions state-specific;
7. secure both live servers, not only the new story server;
8. replace generic lane retry with capture/verification separation;
9. resolve profiles through actual providers and a sanitized environment;
10. defer streaming until ordering, cancellation, and backpressure are defined;
11. add capture-plan digests and strict shard merge validation;
12. reject incomplete evidence in the RC collector;
13. benchmark encoding, runtime, browser memory, and artifact size before
    freezing budgets; and
14. replace brittle catalog constants with plan comparison and add adversarial
    red tests.

No Round 1 finding was rejected.

### Round 2

Both auditors rejected the first revision. All remaining blocker and
high-priority findings were accepted:

1. remove source commit and display labels from the structural plan digest;
2. make safe asynchronous capture the only PTY frame API;
3. replace non-portable directory swapping with immutable bundles and an atomic
   pointer;
4. add per-operation, per-variant, and suite deadlines plus incremental
   journaling;
5. design read-only fork verification into the trusted post-approval RC path;
6. define convergence as three runs per OS and identical hashes across all nine
   runs;
7. reduce V1 styling to public xterm attributes and track cursor modes
   separately;
8. make evidence metadata producer-neutral and distinguish planned from
   exploratory sessions;
9. make metadata the sole owner of capture IDs, labels, and assertions;
10. restrict controls to validated scalar values;
11. choose embedded frames for the self-contained HTML contract; and
12. define supported `tags` and `argTypes` behavior.

No Round 2 finding was rejected.

### Round 3

Architecture/extensibility and reliability/CI/security auditors rejected the
second revision. All blocker and high-priority findings were accepted:

1. make the base-SHA collector and baseline authoritative so candidate code
   cannot approve its own verifier or baseline;
2. reserve job time for setup and fail-closed artifact upload outside capture;
3. derive contained attachment paths in the writer and reject traversal,
   aliases, and symlinks;
4. require explicit story and variant IDs before baseline promotion;
5. reject mixed dirty state, source/runtime/external-input fingerprints, and
   normalization versions during shard merge; and
6. define separate candidate capture, local proposal, and trusted nine-run
   attestation commands.

All lower-priority notes were also incorporated: process-tree cleanup and retry
isolation, repeated convergence for every proposal, typed live comparison
authority, executable sanitation/comparison ordering, and typed sidecar
attachments.

No Round 3 finding was rejected.

### Round 4

Both auditors rejected the third revision with one high-priority finding each:

1. a base-SHA collector could not trust frames produced by a candidate-owned
   capture adapter; and
2. the first baseline could not be bootstrapped in the same PR that introduced
   the collector absent from its base SHA.

Both findings were accepted. The base-SHA visual CLI now owns resolution,
render hosting, PTY capture, checks, sanitation, serialization, and attestation.
Baseline proposals that modify those trusted surfaces are rejected. Rollout is
split into trusted collector landing, nine-run empty-authority bootstrap, and
blocking-gate activation. Convergence sample identity is also separate from
infrastructure retry attempts.

No Round 4 finding was rejected.

### Round 5

Both auditors approved the fourth revision with no blocker or high-priority
finding. The reliability reviewer requested one wording correction to
distinguish nine convergence samples from subordinate retry attempts; it was
applied before final validation.

### Approval Rule

After revision, the same two auditors re-review the plan. It is approved only
when neither reports a blocker or high-priority unmitigated finding. Lower
priority implementation notes remain in this audit record with an owner and
acceptance test.

Round 5 satisfies this approval rule.
