# Local telemetry stack

This stack lets you run `kiro-cli` locally and inspect OpenTelemetry metrics
and logs in Grafana:

```text
kiro-cli ──OTLP/HTTP──▶ otel-collector ──┬─ Prometheus ──▶ Grafana (metrics)
                                         └─ Loki         ──▶ Grafana (logs)
```

## Start the stack with Finch

From this directory:

```bash
cd dev/telemetry
finch compose up -d
```

You can also start it from the repository root with
`finch compose -f dev/telemetry/compose.yaml up -d`.

Grafana is available at http://localhost:3000/d/kiro-telemetry-local/kiro-cli-local-telemetry.
Prometheus is at http://localhost:9090, the collector's Prometheus exporter at
http://localhost:9464/metrics, and Loki at http://localhost:3100.

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

## Inspecting tool usage by name (Loki)

Per-tool counts are not available as Prometheus labels (cardinality control —
the metric `kiro_cli_tool_invocations` only carries `tool_origin` and `outcome`).
The tool name lives on the `kiro_cli_tool_invoked` log record, which is now
shipped to Loki and provisioned as a Grafana datasource.

The dashboard includes:

- **Tool Usage by Name** — time series of per-tool invocation counts.
- **Top Tools (by count, current range)** — bar gauge of the top 15 tools.
- **Tool Outcomes by Name** — invocations split by `tool_name` × `outcome`.
- **Recent Tool Invocations (raw log)** — explore the raw log records and their
  attributes (use the panel's "Inspect → Logs" view to see all fields).

Or query Loki directly in Grafana → Explore → datasource **Loki**:

```logql
# total per tool over the selected range
sum by (tool_name)
  (count_over_time({service_name="kiro-cli"} |= "kiro_cli_tool_invoked" [$__range]))

# only MCP tools, broken down by server
sum by (tool_name, server_name)
  (count_over_time({service_name="kiro-cli", tool_origin="mcp"} |= "kiro_cli_tool_invoked" [$__range]))

# error-only invocations
{service_name="kiro-cli", outcome="error"} |= "kiro_cli_tool_invoked"
```

Loki promotes a wide set of OTLP attributes to labels by default. The event
identity (`kiro_cli_tool_invoked`, `kiro_cli_user_turn_completed`, etc.) lives
in the log **body**, not as a label, so always pair the stream selector with a
line filter (`|= "kiro_cli_tool_invoked"`) to scope to a specific event.
Common useful labels: `tool_name`, `tool_origin`, `outcome`, `model_class`,
`client_application`, `is_subagent`, `os_type`, `service_name`.

## Stop the stack

```bash
cd dev/telemetry
finch compose down
```

Add `-v` if you also want to wipe Prometheus/Loki state between runs.
