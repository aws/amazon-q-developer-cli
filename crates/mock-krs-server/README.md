# mock-krs-server

A fake Kiro Runtime Service (KRS) for deterministic KAS integration tests.

KRS is the model-inference backend KAS calls. Point a real, published
`@kiro/agent` at this server and everything below the model stays real — real
agent loop, real tools, real sessions, real ACP — while the model's output
becomes a value a test wrote down. That is what makes a KAS version bump
verifiable: the only variable left is the KAS artifact itself.

The wire protocol is not written here. Routing, request deserialization,
response serialization and event-stream framing all come from
[`amzn-kiro-runtime-service-server-sdk`](../amzn-kiro-runtime-service-server-sdk/README.md),
generated from `KiroRuntimeServiceModel`. This crate only decides *which*
modeled value to return, so a KRS model change that renames or drops a member is
a compile error rather than a silent loss of test fidelity.

Scope is KAS (v3) and one operation: `GenerateAssistantResponse`, served the way
real KAS addresses it — `awsJson1_0`, so `POST /` with
`x-amz-target: KiroRuntimeService.GenerateAssistantResponse`, bearer auth, and
the whole input as one JSON document.

Two operations KAS also calls on this endpoint, `InvokeMCP` and
`GetFeatureConfiguration`, are **not** served. KAS retries them and continues, so
a scripted turn still completes, but the retries are noise in the logs.

## Two surfaces

| | |
|---|---|
| **Data plane** | The generated service, at `POST /`, dispatching on `x-amz-target`. An operation KRS does not model gets the SDK's own unknown-operation response. |
| **Control plane** | `/__control/*` — the harness's surface: inject scenarios, read captured requests, reset between tests. Namespaced under a path KRS would never serve, so a stray call from KAS can never be mistaken for harness traffic. |

## Quick start: in a Rust test

```rust
use mock_krs_server::{Config, MockKrsServer};
use mock_krs_server::scenario::Turn;
use serde_json::json;

let server = MockKrsServer::start(Config {
    api_key: Some("test-api-key".into()),
    ..Default::default()          // binds 127.0.0.1:0 — a free port per test
}).await?;

server.enqueue(serde_json::from_value::<Vec<Turn>>(json!([{
    "name": "greeting",
    "match": {"userInputContains": "hello"},
    "respond": {"events": [{"type": "text", "content": "Hi there"}]},
}]))?).await;

// hand server.endpoint() to KAS as its --endpoint
let captured = server.requests().await;
assert_eq!(captured[0].matched_turn.as_deref(), Some("greeting"));
server.shutdown().await;
```

`MockKrsServer::start` resolves once the listener is accepting, so there is no
race between starting it and handing the endpoint to KAS. `Drop` releases the
port even if a test panics before `shutdown()`.

## Quick start: standalone

For a harness that is not Rust (the TypeScript E2E suite), or for poking at it
by hand:

```bash
cargo build -p mock-krs-server --bin mock-krs-server
./target/debug/mock-krs-server --port 0 --api-key demo-key --port-file /tmp/krs.port
```

```
[mock-krs] listening on http://127.0.0.1:55763
[mock-krs] set KIRO_KAS_ENDPOINT=http://127.0.0.1:55763
[mock-krs] control API at http://127.0.0.1:55763/__control
```

The `listening on` line is the readiness signal — the same contract
`packages/tui/e2e_tests/cloud/mock-bff.mjs` uses, so a harness can wait for the
line instead of polling. `--port-file` is the alternative for reading the
ephemeral port from disk instead of parsing stdout.

| Flag | |
|---|---|
| `--port` | `0` (default) picks a free port. |
| `--api-key` | Token to require. Also `MOCK_KRS_API_KEY`. Unset accepts any non-empty bearer token. |
| `--port-file` | Write the bound port here once listening. |
| `--allow-unvalidated-requests` | Accept requests carrying no user message. Off by default. |

Script a turn and call it:

```bash
B=http://127.0.0.1:$(cat /tmp/krs.port)

curl -s -X POST $B/__control/scenarios -H 'content-type: application/json' -d '{
  "turns": [{
    "name": "greeting",
    "match": {"userInputContains": "hello"},
    "respond": {"events": [
      {"type": "text", "content": "Hi "},
      {"type": "text", "content": "there"},
      {"type": "metadata", "stopReason": "END_TURN"}
    ]}
  }]}'
# {"queued":1}

curl -s -D- -o /tmp/body.bin -X POST $B/ \
  -H 'authorization: Bearer demo-key' \
  -H 'content-type: application/x-amz-json-1.0' \
  -H 'x-amz-target: KiroRuntimeService.GenerateAssistantResponse' \
  -d '{"conversationState":{"chatTriggerType":"MANUAL",
       "currentMessage":{"userInputMessage":{"content":"hello"}}}}'
```

```
HTTP/1.1 200 OK
content-type: application/x-amz-json-1.0

assistantResponseEvent -> {"content": "Hi "}
assistantResponseEvent -> {"content": "there"}
metadataEvent          -> {"stopReason": "END_TURN"}
```

The body is binary AWS event-stream frames, so `curl` alone will not show them.
`strings /tmp/body.bin` is the quick look; `tests/generate_assistant_response.rs`
has a real decoder worth copying.

## Control API

| Method | Path | |
|---|---|---|
| `GET` | `/__control/health` | `{"ok": true, "queuedTurns": n}` |
| `GET` | `/__control/state` | `{"queuedTurns": [names], "calls": n, "unmatchedCalls": n}` |
| `POST` | `/__control/scenarios` | Enqueue turns. Body is `{"turns": [...]}` or a bare `[...]`. Answers `{"queued": n}`, or `400` naming the offending field. |
| `GET` | `/__control/requests` | Every captured call: `index`, `path`, `body`, `headers`, `matchedTurn`. |
| `POST` | `/__control/reset` | Clear the queue, the capture log and the counters. |

Captured headers are an allowlist, and `authorization` is recorded as
`<redacted>` — the token value is never persisted.

## Scenario format

A scenario is a queue of turns. Each turn pairs an optional request matcher with
the response to play back, so a test can either script a strictly ordered
conversation (no matchers — the first queued turn answers the next call) or bind
responses to the request that should receive them (matchers — order-independent,
which is what multi-turn agent flows need once tool calls make the call order
hard to predict).

Every key is camelCase, and unknown keys are rejected at injection time, which is
the only place a scripting typo can be reported usefully.

### Turn

| Field | Default | |
|---|---|---|
| `name` | `<unnamed>` | Label echoed in `state`, `requests.matchedTurn` and the unscripted-call error. |
| `match` | matches anything | See below. |
| `times` | `1` | How many calls this turn may answer before it is consumed. `0` means unlimited — for background calls that fire an unpredictable number of times (title generation, compaction). |
| `respond` | required | See below. |

### match

All present fields must match (AND). Matching runs against the *modeled* request,
so a matcher cannot drift from the shape KRS accepts.

| Field | |
|---|---|
| `userInputContains` | Substring of `conversationState.currentMessage.userInputMessage.content`. |
| `userInputRegex` | Regex over the same content. An unparseable pattern matches nothing, so a typo cannot answer every request. |
| `bodyContains` | Substring of the raw request body. The escape hatch for asserting on parts of the request the matcher does not model — history shape, advertised tools, images. |
| `hasToolResults` | `true` requires at least one tool result (i.e. the continuation of a tool-using turn); `false` requires none. |
| `modelId` | Exact match on the user message's `modelId`. |
| `agentMode` | Exact match on `agentMode`. Nominally an `@httpHeader` member, but awsJson1_0 ignores HTTP bindings, so it arrives in the document. |
| `callIndex` | Zero-based call index since the last reset. |

### respond

| Field | Default | |
|---|---|---|
| `events` | `[]` | The event sequence, in order. |
| `truncate` | `false` | End the stream after `events` without a terminal stop reason. |
| `streamError` | none | Emit an exception frame instead of finishing. Applied after `events`, so a turn can stream partial content and then fail. |
| `httpError` | none | Fail the request with a modeled error instead of streaming. Takes precedence over `events`. |
| `delayMs` | `0` | Delay before the first byte. |
| `chunkDelayMs` | `0` | Delay between frames — lets a test observe incremental rendering instead of one flush. |

### events

Each entry is `{"type": ..., ...}`.

```jsonc
{"type": "text",         "content": "hi", "modelId": null, "modelTag": null}
{"type": "reasoning",    "text": "thinking", "signature": null, "redactedContent": null}
{"type": "toolUse",      "toolUseId": "tu-1", "name": "fs_read",
                         "input": "{\"path\":", "stop": false}
{"type": "metering",     "usage": 1.5, "unit": "credit", "unitPlural": "credits"}
{"type": "contextUsage", "contextUsagePercentage": 12.5}
{"type": "metadata",     "stopReason": "END_TURN", "tokenUsage": {
                           "uncachedInputTokens": 10, "outputTokens": 4, "totalTokens": 14,
                           "cacheReadInputTokens": null, "cacheWriteInputTokens": null,
                           "contextUsagePercentage": null, "normalizedTokenUsage": null}}
{"type": "delay",        "ms": 50}   // pacing, not an event
```

`toolUse.input` is concatenated by the client across events until one arrives
with `"stop": true`, so splitting a call across frames exercises that
reassembly. `stopReason` must be one the model enumerates — `CONTENT_FILTERED`,
`END_TURN`, `MAX_TOKENS`, `MODEL_CONTEXT_WINDOW_EXCEEDED`, `PAUSE_TURN`,
`TOOL_USE`, `UNKNOWN` — and anything else is rejected as a scripting error
rather than passed through to KAS.

### streamError and httpError

```jsonc
{"streamError": {"kind": "throttling", "message": "slow down"}}
{"httpError":   {"kind": "throttling", "message": "too many requests"}}
```

`streamError.kind` is one of `internal`, `throttling`, `validation`,
`serviceUnavailable`. `httpError.kind` is one of `accessDenied`,
`internalServer`, `serviceQuotaExceeded`, `serviceUnavailable`, `throttling`,
`validation`. Both are closed sets: the mock can only fail the way KRS can fail.
Neither carries a status code — the model decides it, so `throttling` is a `429`
without the scenario saying so.

## Behaviour worth knowing

**A healthy stream always closes with a stop reason.** If a script omits a
terminal `metadata` event, the server appends one with `END_TURN`. Real KRS
always closes a complete response that way, and a stream that ends without one
trips KAS's stream-recovery retry, which silently doubles invocation counts and
makes a test look flaky rather than wrong. Use `truncate` to ask for the other
behaviour deliberately.

**An unscripted call is fatal.** No stubbed default: the server answers `500`
with a message naming the call index, the user input and the turns still queued.

```
mock KRS has no scripted response for call 1 (user input: "hello"); queued turns: []
```

A default would let an incorrectly scripted test pass while asserting nothing,
which is the failure mode a version-bump gate cannot afford. The unmatched
request is still captured, so the test can show what was actually sent.

**A bearer token is always required**, even with no `--api-key`. KAS always
sends one, so its absence is a wiring bug worth surfacing. A missing or wrong
token is `403`.

**Requests without a user message are rejected** with `400`, which catches gross
request-shape breakage at the first call instead of as a stalled turn. Disable
with `--allow-unvalidated-requests` / `Config::validate_requests`.

## Against a real KAS

Verified with published `@kiro/agent@0.35.11`. KAS takes the endpoint as a CLI
flag only — there is no env fallback on its side — and `--endpoint=<url>` must
use the `=` form:

```bash
node --experimental-wasm-modules \
  packages/tui/node_modules/@kiro/agent/dist/server/acp-server.js \
  --transport=stdio --endpoint=http://127.0.0.1:<port>
```

Set `KIRO_API_KEY` to the mock's `--api-key`: it short-circuits KAS's auth
provider selection ahead of every `--auth` mode, so no token file or ACP callback
is needed. Then drive ACP over stdio (`initialize`, `session/new`,
`session/prompt`) and a scripted turn arrives as an `agent_message_chunk`:

```
  <agent_message_chunk> Hello from the fake KRS!
[kas] [INFO] [KRS] <-- GenerateAssistantResponseCommand done totalEvents=2
stopReason: {"stopReason":"end_turn"}
```

Through the kiro-cli TUI instead of raw ACP, the endpoint is plumbed by
`KIRO_KAS_ENDPOINT` (PR #3934), which appends `--endpoint` at the spawn site in
`packages/tui/src/acp-client/kas.ts`.

## Tests

```bash
cargo test -p mock-krs-server
```

26 unit tests, 5 control-API tests, and 14 that drive the server over raw HTTP
and decode the event-stream frames. The integration tests deliberately do not use
a generated client: the bytes are the contract KAS's TypeScript client consumes,
so asserting on them proves the mock is wire-correct instead of proving that one
codegen output agrees with another.

## Limits

* The generated SDK does not enforce presence or value constraints, because the
  KRS model's constraint traits had to be stripped to make server codegen run at
  all. See the [SDK README](../amzn-kiro-runtime-service-server-sdk/README.md).
* A field KAS adds to the request is silently ignored, as it would be by real
  KRS: JSON deserializers skip unknown members. Catching additive drift needs a
  golden-request comparison, which does not exist yet.
* `InvokeMCP` and `GetFeatureConfiguration` are not served, so KAS retries them
  and moves on.
* KAS also calls the control-plane service (`GetProfile`, `ListAvailableModels`,
  `GetUsageLimits`) on a separate endpoint behind its own
  `--control-plane-endpoint` flag. This mock does not fake those, so they reach
  the real service.
* No `conversationId` in the response — the modeled member had to be dropped for
  codegen to accept a streaming output under awsJson1_0.
