# ToolSearch/ToolSearch Eval Harness

End-to-end evaluation of the ToolSearch/ToolSearch feature. Runs scenarios through a real agent with live MCP servers and records tool loading decisions, MCP calls, and final responses.

## Build

```bash
cargo build -p chat_cli_v2 --example tool_search_eval
```

## Usage

```bash
# Run all scenarios (from repo root)
./target/debug/examples/tool_search_eval --agent office

# Run a specific scenario
./target/debug/examples/tool_search_eval --agent office --scenario-id cat2_chained

# Multiple runs for consistency testing
./target/debug/examples/tool_search_eval --agent office --scenario-id cat2_chained --runs 10
```

### Options

| Flag | Default | Description |
|------|---------|-------------|
| `--agent` | (required) | Agent config name (e.g. `office`) |
| `--scenarios` | `crates/chat-cli-v2/eval/eval_scenarios.json` | Path to scenarios file |
| `--scenario-id` | (all) | Run only this scenario |
| `--runs` | `1` | Number of times to run each scenario |

## Scenarios

Defined in `eval_scenarios.json`. Each scenario has:

- `id` — unique identifier (e.g. `cat1_exact_intent`)
- `query` — the user prompt to send
- `expected_tool` — what tool should be loaded/called (or `"none"` for negative tests)
- `verification` — pass/fail criteria

### Current scenarios

| ID | Category | Query |
|----|----------|-------|
| `cat1_exact_intent` | Exact intent | Phone tool lookup |
| `cat2_chained` | Chained reasoning | Sprint task count (requires multiple tools) |
| `cat3_negative_trap` | Negative/trap | Delete code review (no such tool exists) |

## Output

Results are written as JSONL to `output/{scenario_id}.jsonl` (one JSON object per line). The output directory is gitignored. Results append across runs.

Each result contains:
- `scenario_id`, `run`, `query`
- `tool_search_calls` — which tools were loaded and what was returned
- `mcp_tool_calls` — which MCP tools were actually called
- `final_response` — the agent's text response
- `error` — error message if the run failed
