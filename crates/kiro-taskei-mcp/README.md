# kiro-taskei-mcp

MCP stdio server that will back the kiro-help bot's Taskei-side tools. Tracks
the Phase 1 plan in
[`docs/superpowers/plans/2026-05-29-kiro-bot-taskei-mcp-integration.md`](../../docs/superpowers/plans/2026-05-29-kiro-bot-taskei-mcp-integration.md).

## Status: Phase 1c (signed initialize probe with STS bridge)

The binary now signs and sends a real MCP `initialize` POST to the
configured Taskei endpoint and exits with the gateway's response status.
With both `--read-role-arn` and `--write-role-arn` unset (the Phase-0
reality, where Taskei accepts the kiro-bot ECS task role directly), the
STS bridge is a no-op pass-through and the call signs with the same base
creds Phase 1b used. When a read role is configured, the read path goes
through a cached `AssumeRoleProvider`. When `--scope=write` and a write
role is configured, the binary builds a one-shot signed client from a
per-call `AssumeRole` snapshot and drops the provider after the single
signed call.

Phases still ahead:

- **1d** — `rmcp::ServerHandler` exposing the read tool set (get/list
  rooms, get/list tasks) with `readOnlyHint` annotations; write tools
  (`Taskei___create_task`, `Taskei___update_task`) gate on
  `--scope=write` and the room allowlist.

## CLI

```
kiro-taskei-mcp [OPTIONS]

Options:
  --region <REGION>                AWS region [env: AWS_REGION=] (default us-east-1)
  --endpoint <URL>                 Override Taskei endpoint [env: KIRO_TASKEI_ENDPOINT=]
  --scope <read|write>             Default: read
  --read-role-arn <ARN>            STS role for read calls [env: KIRO_TASKEI_READ_ROLE_ARN=]
  --write-role-arn <ARN>           STS role for write calls [env: KIRO_TASKEI_WRITE_ROLE_ARN=]
  --allow-rooms <ID,ID,...>        Room ID allowlist [env: KIRO_TASKEI_ALLOW_ROOMS=]
```

## Credentials

This binary takes **no** `--aws-profile` flag. Credentials come from the AWS
default provider chain:

- **In ECS** — the task role attached to the kiro-bot service.
- **Locally** — whatever the ambient environment resolves. Use
  `AWS_PROFILE=kiro-bot` (or whichever profile holds Taskei access) when
  invoking the binary by hand:

  ```bash
  AWS_PROFILE=kiro-bot kiro-taskei-mcp --scope read
  ```

Setting `--read-role-arn` tells the binary to `sts:AssumeRole` from those
base credentials before signing read-tool calls; the assumed snapshot is
cached in-process (5-min refresh margin, 50-min hard recycle).
`--write-role-arn` causes write-tool calls to assume the write role
*per invocation* — the `AssumeRoleProvider` is built fresh, resolved
once, and dropped before the signed call returns.

## See also

- Sister crates: [kiro-knowledge-mcp](../kiro-knowledge-mcp/),
  [kiro-github-mcp](../kiro-github-mcp/).
- Design doc:
  [`docs/superpowers/plans/2026-05-29-kiro-bot-taskei-mcp-integration.md`](../../docs/superpowers/plans/2026-05-29-kiro-bot-taskei-mcp-integration.md).
