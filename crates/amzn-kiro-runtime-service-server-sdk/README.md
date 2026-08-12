# amzn-kiro-runtime-service-server-sdk

Generated Rust **server** SDK for the Kiro Runtime Service (KRS), vendored here
so `mock-krs-server` can serve KRS's wire protocol without hand-writing any of
it. Do not edit the sources: regenerate them.

There is no Brazil package that publishes this artifact — `KiroRuntimeServiceRustClient`
generates a *client*, and nothing generates a server — so it is produced locally
with the recipe below and committed.

## Provenance

| | |
|---|---|
| Model | `KiroRuntimeServiceModel` (Brazil), service `com.amazon.kiro.runtimeservice#KiroRuntimeService` |
| Version set | `KiroRuntimeService/development` |
| Codegen | smithy-rs `rust-server-codegen` via `SmithyBrazilRust` / `SmithyRustAwsCodegen` |
| Codegen revision | `08a7653a9f53fff437fabec7b86a846eca1be211` (see `package.metadata.smithy` in `Cargo.toml`) |
| Protocol | `aws.protocols#awsJson1_0` — `POST /` with `x-amz-target` |

The protocol matters, and it is not the one the model reads as its default. KRS
carries both `@restJson1` and `@awsJson1_0`, and server codegen picks restJson1
on its own — but **real KAS speaks awsJson1_0**. Verified two ways: the KRS
client bundled inside `@kiro/agent` sets `protocol: AwsJson1_0Protocol` with
`serviceTarget: "KiroRuntimeService"`, and live traffic from KAS 0.35.11 arrives
as `POST /` with `content-type: application/x-amz-json-1.0` and
`x-amz-target: KiroRuntimeService.GenerateAssistantResponse`.

Do not trust the standalone `@amzn/kiro-runtime-service-typescript-client@1.0.0`
in `node_modules` as evidence — that copy is generated for restJson1, a newer
protocol choice than the one KAS 0.35.11 was built against. Reading it instead of
the bundle is how this crate first got generated for the wrong protocol. A KAS
bump that flips the protocol is exactly the kind of drift this mock exists to
catch, so re-check it when bumping.

## Regenerating

```bash
brazil workspace create --name krs-server-sdk --versionSet KiroRuntimeService/development
cd krs-server-sdk
brazil workspace use -p KiroRuntimeServiceRustClient
cd src/KiroRuntimeServiceRustClient
# replace smithy-build-template.json with the one below
PATH="$HOME/.cargo/bin:$PATH" brazil-build release
cp -R amzn-kiro-runtime-service-server-sdk/{src,Cargo.toml} \
  <kiro-cli>/crates/amzn-kiro-runtime-service-server-sdk/
```

On macOS the `PATH` prefix is required: the Brazil-vended `cargo` is a Linux
binary, and `path-allowing-local-dev` only falls back to a local one it can find
on `PATH`.

After copying, two mechanical steps before committing:

```bash
# 1. Drop the Brazil-only keys from the generated Cargo.toml (`exclude`,
#    `publish`) so it works as a plain workspace member.
# 2. Strip trailing whitespace, then format. Codegen emits a couple of
#    whitespace-only lines that make rustfmt fail with an internal error
#    ("left behind trailing whitespace"), and CI runs `cargo +nightly fmt
#    --check` over the whole workspace. The other vendored SDKs in this repo are
#    stored formatted for the same reason.
perl -i -pe 's/[ \t]+$//' crates/amzn-kiro-runtime-service-server-sdk/src/**/*.rs
cargo +nightly fmt -p amzn-kiro-runtime-service-server-sdk
```

## The generation config, and why each transform is there

```json
{
  "version": "1.0",
  "projections": {
    "kiro-runtime-service": {
      "imports": [
        "KiroRuntimeServiceModel"
      ],
      "transforms": [
        {
          "name": "excludeShapesBySelector",
          "args": {
            "selector": ":is([id=com.amazon.kiro.runtimeservice#GenerateVoiceAssistantResponse], [id=com.amazon.kiro.runtimeservice#InvokeMCP], [id=com.amazon.kiro.runtimeservice#InvokeMCPStream], [id=com.amazon.kiro.runtimeservice#GenerateCompletions], [id=com.amazon.kiro.runtimeservice#CreateResponse], [id=com.amazon.kiro.runtimeservice#GetFeatureConfiguration], [id=com.amazon.kiro.runtimeservice#TranscribeVoice], [id=com.amazon.kiro.runtimeservice#SynthesizeVoice])"
          }
        },
        {
          "name": "excludeShapesBySelector",
          "args": {
            "selector": "[id='com.amazon.kiro.runtimeservice#GenerateAssistantResponseResponse$conversationId']"
          }
        },
        {
          "name": "removeUnusedShapes"
        },
        {
          "name": "renameShapes",
          "args": {
            "renamed": {
              "com.amazon.kiro.runtimeservice#ValidationException": "com.amazon.kiro.runtimeservice#ValidationError"
            }
          }
        },
        {
          "name": "excludeTraits",
          "args": [
            "aws.protocols#restJson1",
            "smithy.api#http",
            "smithy.api#httpPayload",
            "smithy.api#httpHeader",
            "smithy.api#httpLabel",
            "smithy.api#httpQuery",
            "smithy.api#httpResponseCode",
            "smithy.api#length",
            "smithy.api#pattern",
            "smithy.api#range",
            "smithy.api#required"
          ]
        }
      ],
      "plugins": {
        "rust-server-codegen": {
          "service": "com.amazon.kiro.runtimeservice#KiroRuntimeService",
          "codegen": {
            "publicConstrainedTypes": false,
            "http-1x": true
          },
          "module": "amzn-kiro-runtime-service-server-sdk",
          "moduleDescription": "Rust server SDK for the Kiro Runtime Service",
          "moduleVersion": "1.0.0",
          "moduleAuthors": [
            "Kiro Team"
          ]
        }
      }
    }
  }
}
```

**First `excludeShapesBySelector` + `removeUnusedShapes`** — keeps only
`GenerateAssistantResponse`, the sole operation the mock serves. It also avoids
the voice operations, whose `@streaming` blob members become invalid once
`@required` is stripped.

Real KAS also calls `InvokeMCP` (a JSON-RPC `tools/list` in the body) and
`GetFeatureConfiguration` on this same endpoint. They are excluded here, so the
mock refuses them; KAS retries and carries on. Serving them means putting them
back in this selector and implementing handlers.

**Second `excludeShapesBySelector`** — drops
`GenerateAssistantResponseResponse$conversationId`. smithy-rs allows only the
event-stream member in an output that streams ("We only support one payload
member if that payload contains a streaming member", smithy-rs#2237), and under
awsJson1_0 the extra member trips that rule even with the HTTP binding traits
removed. `alwaysSendEventStreamInitialResponse` does not help. The mock therefore
returns no conversation id — KAS 0.35.11 does not require one.

**`renameShapes`** — KRS models its own `ValidationException`. Server codegen
auto-injects `smithy.framework#ValidationException` for operations with
constrained input, and two shapes of that name collide in the generated crate
(~49 compile errors). Renaming KRS's to `ValidationError` resolves it. Consequence
to know about: KRS's own validation error now serializes as `ValidationError`, so
`mock-krs-server` returns the *framework* `ValidationException` when it wants the
code KAS's client recognizes. The vendored client used the same rename.

**`excludeTraits`, protocol and HTTP bindings** — `aws.protocols#restJson1` is
removed so codegen resolves awsJson1_0 rather than preferring restJson1, and the
`@http*` traits go with it: awsJson ignores HTTP bindings, and leaving them on
keeps the payload resolver treating the event stream as an HTTP payload. One
visible consequence is that `agentMode`, nominally an `@httpHeader`, arrives as
an ordinary document member — which is what real KAS sends.

**`excludeTraits`, constraints** — constraint traits on shapes inside an event stream are a
hard codegen abort (`canBeIgnored = false`, smithy#1388), and `ignoreUnsupportedConstraints`
does not cover them. KRS puts `@required`, `@length`, `@pattern` and `@range` all
over the `ChatResponseStream` closure, several on shapes shared with the request,
so they come off model-wide. Every generated member is therefore `Option`, which
costs nothing for a mock but means the SDK does not enforce presence.

**`http-1x`** — without it the crate generates against `aws-smithy-legacy-http-server`
(hyper 0.14 / http 0.2), which cannot share a router with the axum 0.8 control
plane in `mock-krs-server`.

**`publicConstrainedTypes: false`** — keeps plain `String`/`i32` in the public API
instead of constrained newtypes.

## Known divergence from real KRS

* Presence and value constraints are not enforced (see `excludeTraits`).
* `ValidationError` in place of KRS's `ValidationException` (see `renameShapes`).
* No `conversationId` in the response (see the second `excludeShapesBySelector`).
* Only `GenerateAssistantResponse` exists. `InvokeMCP` and
  `GetFeatureConfiguration`, which KAS does call, get the SDK's
  unknown-operation response.
