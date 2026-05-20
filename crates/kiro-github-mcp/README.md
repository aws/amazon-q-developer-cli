# kiro-github-mcp

MCP stdio server that backs the kiro-help bot's GitHub-side tools:

- `search_github_issues` — read-only keyword search.
- `create_github_issue`  — write: opens an issue (gated by the bot's
  reaction-approval flow before invocation).
- `comment_on_existing`  — write: posts a comment on an issue or PR.

Read-only and write-capable scopes are split into two distinct invocations of
the same binary so each can carry a different PAT:

```bash
# Read-only — bot launches this all the time
GH_PAT=$KIRO_BOT_GH_PAT_READ kiro-github-mcp --scope read

# Write — bot launches this only when an approval is granted
GH_PAT=$KIRO_BOT_GH_PAT_WRITE kiro-github-mcp --scope write
```

## Rate limiting

A 30 calls/min token bucket is shared across all three tools. The bot's
PAT-quota fence sits in front of the GitHub side, so even with a write-PAT
that can comment everywhere, a runaway agent loop will get rate-limited
after 30 invocations and surface a clean error to the user instead of a
secondary-rate-limit response from GitHub.

## CLI

```
kiro-github-mcp [OPTIONS]

Options:
  --scope <read|write>          Default: read
  --token <PAT>                 [env: GH_PAT=]
  --default-repo <OWNER/REPO>   [env: KIRO_GITHUB_DEFAULT_REPO=] (default kiro-team/kiro-cli)
```

## See also

- Spec: [docs/superpowers/specs/2026-05-13-kiro-help-bot-design.md](../../docs/superpowers/specs/2026-05-13-kiro-help-bot-design.md) — sec 6 tools table.
- Sister crate: [kiro-knowledge-mcp](../kiro-knowledge-mcp/) — the kiro-help bot's RAG retrieval surface.
