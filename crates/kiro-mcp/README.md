# kiro-mcp

Bundled MCP stdio shim for kiro-bot. Currently hosts the **Taskei** tool
family; **knowledge** and **github** migrate in via a follow-up
consolidation plan (open question §6 in the integration plan). Tracks
the Phase 1 plan in
[`docs/superpowers/plans/2026-05-29-kiro-bot-taskei-mcp-integration.md`](../../docs/superpowers/plans/2026-05-29-kiro-bot-taskei-mcp-integration.md).

## Status: Phase 1d (rmcp stdio proxy + tool annotations + schema pin)

Phase 1d stands up an `rmcp::ServerHandler` that proxies kiro-help's
`tools/list` and `tools/call` requests to the IAD prod Taskei MCP
gateway over a SigV4-signed JSON-RPC HTTPS POST. The 7-tool curated
catalog is decorated with per-tool `readOnlyHint` / `destructiveHint`
annotations from the plan's overlay table (§377-384). At process
startup the binary runs a **schema-pin assertion**: it signs a
`tools/list` against the live gateway, normalizes the response
(canonical-JSON SHA-256 of each tool's `inputSchema`, name-keyed
sorted map), and compares it to a checked-in fixture under
`src/families/taskei/fixtures/taskei-tools.json`. Mismatch emits one
audit-log line per discrepancy on the `taskei_audit` target and
exits the process non-zero, fail-closed against gateway drift.

Read tools (`Taskei___list_tasks`, `_get_task`, `_get_room`,
`_list_room_resource`, plus the `x_amz_bedrock_agentcore_search`
helper) sign with the bridge's read provider — cached read-role
creds when configured, base task-role creds otherwise. Write tools
(`Taskei___create_task`, `Taskei___update_task`) take a *per-call*
STS AssumeRole snapshot via `assume_write_once`, sign exactly one
HTTPS POST with it, and drop the snapshot before returning.

The Phase-0 reality (both ARNs unset, kiro-bot task role accepted
directly as a Taskei room member) is the no-op default and remains
bit-for-bit equivalent to Phase 1b's signing path. Reads and writes
fall through to base credentials in that case; the audit log
records `mode=base`.

The legacy binary name `kiro-taskei-mcp` survives as a deprecated
alias for one release so any in-flight `kiro-help.json` entry or
local script keeps working until Phase 2's agent-config switch
lands. Invoking the alias logs a structured deprecation warning on
the `taskei_audit` target. The alias is removed in Phase 2.

Phases still ahead:

- **2** — kiro-help agent.json wires `kiro-mcp` as the bundled MCP
  server entry; read tools added to the agent's `allowedTools` so
  they auto-approve. Write tools stay out of the allowlist so the
  reaction gate fires per plan §437-461. Alias binary removed.
- **3a** — Bedrock-embedding dedup ranker (no writes).

## CLI

```
kiro-mcp [OPTIONS]

Options:
  --region <REGION>                AWS region [env: AWS_REGION=] (default us-east-1)
  --endpoint <URL>                 Override Taskei endpoint [env: TASKEI_ENDPOINT=]
  --scope <both|read-only>         Phase 1d break-glass; default `both`
  --read-role-arn <ARN>            STS role for read calls [env: TASKEI_READ_ROLE_ARN=]
  --write-role-arn <ARN>           STS role for write calls [env: TASKEI_WRITE_ROLE_ARN=]
  --allow-rooms <ID,ID,...>        Room ID allowlist [env: TASKEI_ALLOW_ROOMS=]
  --enabled-families <NAME,...>    Tool families to expose (Phase 1c-bundle: only `taskei`)
                                   [env: KIRO_MCP_ENABLED_FAMILIES=, default: taskei]
```

The legacy `KIRO_TASKEI_*` environment variable names are accepted as
fallbacks for one release, but the bot config contract uses the
unprefixed `TASKEI_*` names above.

## Credentials

This binary takes **no** `--aws-profile` flag. Credentials come from the AWS
default provider chain:

- **In ECS** — the task role attached to the kiro-bot service.
- **Locally** — whatever the ambient environment resolves. Use
  `AWS_PROFILE=kiro-bot` (or whichever profile holds Taskei access) when
  invoking the binary by hand:

  ```bash
  AWS_PROFILE=kiro-bot kiro-mcp
  ```

Setting `--read-role-arn` tells the binary to `sts:AssumeRole` from those
base credentials before signing read-tool calls; the assumed snapshot is
cached in-process (5-min refresh margin, 50-min hard recycle).
`--write-role-arn` causes write-tool calls to assume the write role
*per invocation* — the `AssumeRoleProvider` is built fresh, resolved
once, and dropped before the signed call returns.

## Schema-pin fixture

The fixture at `src/families/taskei/fixtures/taskei-tools.json` is
the contract this shim ships with. To refresh it after a Taskei
gateway change, run the dumper binary:

```bash
AWS_PROFILE=kiro-bot AWS_REGION=us-east-1 \
  cargo run -p kiro-mcp --bin dump-taskei-tools
```

Re-running produces deterministic bytes — re-review the diff in your
PR. The runtime assertion is fail-closed; if you need to ship a
binary against a gateway that hasn't matched yet, set
`KIRO_MCP_SKIP_SCHEMA_PIN=1` as a documented break-glass and refresh
the fixture as soon as you can.

## Read/write boundary

The `families/<x>/read/` modules take a `kiro_mcp::sts_bridge::ReadOnlyView`
and never import the bridge's write helper directly. The view exposes
only `read_credentials_provider()`, so a read tool *cannot* assume the
write role even if it tried — the type doesn't have the method. The
trybuild compile-fail fixture under `tests/compile_fail/` asserts the
boundary at build time; if a future refactor exposes
`assume_write_once` on `ReadOnlyView`, that test goes green and CI
catches the regression.

## See also

- Sister crates: [kiro-knowledge-mcp](../kiro-knowledge-mcp/),
  [kiro-github-mcp](../kiro-github-mcp/) (consolidation into this crate
  tracked separately).
- Design doc:
  [`docs/superpowers/plans/2026-05-29-kiro-bot-taskei-mcp-integration.md`](../../docs/superpowers/plans/2026-05-29-kiro-bot-taskei-mcp-integration.md).
