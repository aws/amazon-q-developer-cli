# kiro-knowledge-mcp

MCP stdio server backed by an Amazon Bedrock Knowledge Base. Exposes one tool — `search_kiro_knowledge` — for use by the kiro-help bot.

## Build

```bash
cargo build --release -p kiro-knowledge-mcp
```

## Run (stub mode, for local testing)

```bash
kiro-knowledge-mcp --stub
```

Returns a fixed canned response. No AWS credentials required. Used by integration tests and for verifying MCP wire-up without standing up Bedrock.

## Run (production)

```bash
kiro-knowledge-mcp --kb-id ABC123XYZ --region us-west-2
```

Or via env vars:

```bash
KIRO_KNOWLEDGE_KB_ID=ABC123XYZ AWS_REGION=us-west-2 kiro-knowledge-mcp
```

The process uses the AWS default credential chain (env vars, profile, EC2/ECS task role).

## Tool: `search_kiro_knowledge`

```json
{
  "name": "search_kiro_knowledge",
  "inputSchema": {
    "type": "object",
    "properties": {
      "query":         { "type": "string" },
      "source_filter": { "type": "string", "enum": ["docs","issues","releases","all"], "default": "all" },
      "max_results":   { "type": "integer", "default": 5 }
    },
    "required": ["query"]
  }
}
```

Returns the top-N retrieved chunks formatted as numbered citations:

```
[1] docs/auth.md (relevance: 0.91)
    Run kiro-cli login. ...

[2] github_issue:kiro-team/kiro-cli#42 (relevance: 0.70)
    Login hangs on Linux. ...
```

## IAM

The host process needs:

- `bedrock:Retrieve` on the target Knowledge Base ARN.

That is the only AWS permission this binary uses.

## See also

- Spec: [docs/superpowers/specs/2026-05-13-kiro-help-bot-design.md](../../docs/superpowers/specs/2026-05-13-kiro-help-bot-design.md)
- Existing MCP server pattern: [crates/mock-mcp-server/](../mock-mcp-server/)
