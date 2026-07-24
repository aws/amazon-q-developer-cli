# Local telemetry stack

This stack lets you run `kiro-cli` locally and inspect OpenTelemetry metrics
in Grafana:

```text
kiro-cli ──OTLP/HTTP──▶ otel-collector ──▶ Prometheus ──▶ Grafana (metrics)
```

The local stack is **metrics-only** (Prometheus + Grafana). KUTS, the
production backend, does not support OTLP logs, so there is no log pipeline to
mirror locally.

## Start the stack with Finch

From this directory:

```bash
cd dev/telemetry
finch compose up -d
```

You can also start it from the repository root with
`finch compose -f dev/telemetry/compose.yaml up -d`.

Grafana is available at http://localhost:3000/d/kiro-telemetry-local/kiro-cli-local-telemetry.
Prometheus is at http://localhost:9090 and the collector's Prometheus exporter
is at http://localhost:9464/metrics.

To smoke-test the collector → Prometheus path before launching Kiro:

```bash
bash smoke-test.sh
```

To smoke-test the actual `kiro-telemetry` Rust emitter against the same stack:

```bash
bash smoke-kiro-telemetry.sh
```

To emit one record for **every** metric and log in the §5 catalog and assert each
metric instrument is observed in Prometheus (the catalog-coverage gate):

```bash
bash verify-catalog.sh
```

The same record set is asserted in CI by the `catalog_coverage` integration test
(`cargo test -p kiro-telemetry --features test-support --test catalog_coverage`),
which fails if any non-derived catalog metric lacks a typed constructor.

This stack is also exercised end-to-end by CI: the `tui-telemetry-e2e` job in
`.github/workflows/tui.yml` brings the stack up with `docker compose` and runs
`validate-metrics-e2e.sh`, which runs the real V1 binary in dual-write mode,
records both its OTLP and legacy Toolkit requests, then drives the Rust catalog
emitter and TUI fixture (`emit-tui-metrics.fixture.ts`) through their real code
paths. It runs whenever telemetry-relevant paths change, so these scripts are
part of the test suite, not dev-only cruft.

## Run Kiro against the local collector

Build the TUI bundle first:

```bash
cd packages/tui
bun run build
cd ../..
```

Run V2 with the Rust agent engine:

```bash
KIRO_TELEMETRY_ENABLED=1 \
KIRO_TELEMETRY_OTEL=2 \
KIRO_TELEMETRY_OTLP_ENDPOINT=http://localhost:4318 \
KIRO_TELEMETRY_EXPORT_INTERVAL_MS=5000 \
KIRO_TEST_TUI_JS_PATH=$(pwd)/packages/tui/dist/tui.js \
cargo run -p chat_cli --bin chat_cli -- chat --tui
```

Run V2 with KAS:

```bash
KIRO_TELEMETRY_ENABLED=1 \
KIRO_TELEMETRY_OTEL=2 \
KIRO_TELEMETRY_OTLP_ENDPOINT=http://localhost:4318 \
KIRO_TELEMETRY_EXPORT_INTERVAL_MS=5000 \
KIRO_TEST_TUI_JS_PATH=$(pwd)/packages/tui/dist/tui.js \
cargo run -p chat_cli --bin chat_cli -- chat --agent-engine=kas
```

`KIRO_TELEMETRY_EXPORT_INTERVAL_MS=5000` shortens the OpenTelemetry metric export interval
for local development. The production default remains 60 seconds.

The Rust CLI still honors the normal privacy gates. If telemetry is disabled in settings or with
`KIRO_DISABLE_TELEMETRY`, the local stack should not receive product telemetry.

## Check that data arrived

After starting a chat and sending at least one prompt, refresh the Grafana dashboard or query
Prometheus directly:

```bash
curl -G -s http://localhost:9090/api/v1/query \
  --data-urlencode 'query={job="otel-collector", __name__=~"cli_session.*|chat_session.*|chat_cli.*|kiro_cli.*"}'
```

The collector also prints detailed OTLP metric and log payloads:

```bash
finch compose logs -f otel-collector
```

## Stop the stack

```bash
cd dev/telemetry
finch compose down
```

Add `-v` if you also want to wipe Prometheus state between runs.
