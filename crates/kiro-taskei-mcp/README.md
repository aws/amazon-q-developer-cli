# kiro-taskei-mcp

MCP stdio server that will back the kiro-help bot's Taskei-side tools. Tracks
the Phase 1 plan in
[`docs/superpowers/plans/2026-05-29-kiro-bot-taskei-mcp-integration.md`](../../docs/superpowers/plans/2026-05-29-kiro-bot-taskei-mcp-integration.md).

## Status: Phase 1a (stub)

This binary is intentionally a no-op today. It parses its CLI surface,
initializes tracing, prints a startup line, and exits 0. No HTTP, no SigV4,
no STS, no MCP server is wired up yet.

Phases that will fill it in:

- **1b** — SigV4-signed Taskei HTTP client with the ambient default provider
  chain.
- **1c** — STS `AssumeRole` for `--read-role-arn` / `--write-role-arn` so the
  bot can sign with a least-privileged downstream identity.
- **1d** — `rmcp::ServerHandler` exposing the read tool set (get/list rooms,
  get/list tasks); write tools (`create_task`, `update_task`) gate on
  `--scope=write` and the room allowlist.

Even as a stub it ships in the kiro-bot binary-carrier image so the runtime
container picks it up the moment 1b lands.

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

Setting `--read-role-arn` / `--write-role-arn` (Phase 1c) tells the binary to
`sts:AssumeRole` from those base credentials before signing requests.

## See also

- Sister crates: [kiro-knowledge-mcp](../kiro-knowledge-mcp/),
  [kiro-github-mcp](../kiro-github-mcp/).
- Design doc:
  [`docs/superpowers/plans/2026-05-29-kiro-bot-taskei-mcp-integration.md`](../../docs/superpowers/plans/2026-05-29-kiro-bot-taskei-mcp-integration.md).
