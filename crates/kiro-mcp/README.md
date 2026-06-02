# kiro-mcp

Bundled MCP stdio shim for kiro-bot. Currently hosts the **Taskei** tool
family; **knowledge** and **github** migrate in via a follow-up
consolidation plan (open question §6 in the integration plan). Tracks
the Phase 1 plan in
[`docs/superpowers/plans/2026-05-29-kiro-bot-taskei-mcp-integration.md`](../../docs/superpowers/plans/2026-05-29-kiro-bot-taskei-mcp-integration.md).

## Status: Phase 1c-bundle (rename + families/ scaffolding + read/write boundary)

Phase 1c-bundle finishes the rename `crates/kiro-taskei-mcp/` →
`crates/kiro-mcp/` (plan §2 / §317), lays down `families/<x>/{read,write}/`
module scaffolding, and adds a trybuild compile-fail fixture asserting
that read modules cannot import the write-role STS helper. The
signed-initialize probe shipped in Phase 1b/1c remains the deploy-time
smoke until 1d's `tools/list` schema-pin replaces it. No behavioral
change to the SigV4 client or STS bridge.

The legacy binary name `kiro-taskei-mcp` survives as a deprecated alias
for one release so any in-flight `kiro-help.json` entry or local script
keeps working until Phase 2's agent-config switch lands. Invoking the
alias logs a structured deprecation warning on the `taskei_audit`
target. The alias is removed in Phase 2.

Phases still ahead:

- **1d** — `rmcp::ServerHandler` exposing the read tool set (get/list
  rooms, get/list tasks) with `readOnlyHint` annotations; write tools
  (`Taskei___create_task`, `Taskei___update_task`) gate on
  `--scope=write` and the room allowlist; startup schema-pin assertion
  against a checked-in `tools/list` fixture.

## CLI

```
kiro-mcp [OPTIONS]

Options:
  --region <REGION>                AWS region [env: AWS_REGION=] (default us-east-1)
  --endpoint <URL>                 Override Taskei endpoint [env: KIRO_TASKEI_ENDPOINT=]
  --scope <read|write>             Default: read
  --read-role-arn <ARN>            STS role for read calls [env: KIRO_TASKEI_READ_ROLE_ARN=]
  --write-role-arn <ARN>           STS role for write calls [env: KIRO_TASKEI_WRITE_ROLE_ARN=]
  --allow-rooms <ID,ID,...>        Room ID allowlist [env: KIRO_TASKEI_ALLOW_ROOMS=]
  --enabled-families <NAME,...>    Tool families to expose (Phase 1c-bundle: only `taskei`)
                                   [env: KIRO_MCP_ENABLED_FAMILIES=, default: taskei]
```

## Credentials

This binary takes **no** `--aws-profile` flag. Credentials come from the AWS
default provider chain:

- **In ECS** — the task role attached to the kiro-bot service.
- **Locally** — whatever the ambient environment resolves. Use
  `AWS_PROFILE=kiro-bot` (or whichever profile holds Taskei access) when
  invoking the binary by hand:

  ```bash
  AWS_PROFILE=kiro-bot kiro-mcp --scope read
  ```

Setting `--read-role-arn` tells the binary to `sts:AssumeRole` from those
base credentials before signing read-tool calls; the assumed snapshot is
cached in-process (5-min refresh margin, 50-min hard recycle).
`--write-role-arn` causes write-tool calls to assume the write role
*per invocation* — the `AssumeRoleProvider` is built fresh, resolved
once, and dropped before the signed call returns.

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
