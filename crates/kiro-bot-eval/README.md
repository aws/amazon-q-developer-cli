# kiro-bot-eval

Offline RAG retrieval eval for the kiro-help bot. Runs a hand-curated set of
`{query, expected source paths}` cases through a `Retriever` (real Bedrock KB
or a stub) and reports recall@N. Used as a CI gate in Phase 5: a regression in
the corpus or in retrieval blocks the next image publish.

## Run locally

```bash
KB_ID=$(AWS_PROFILE=kiro-bot aws --region us-east-1 ssm get-parameter \
  --name /kiro-help-bot/alpha/kb-id --query Parameter.Value --output text)

AWS_PROFILE=kiro-bot cargo run --release -p kiro-bot-eval -- \
  --kb-id "$KB_ID" \
  --top-n 5 \
  --threshold 0.7
```

The binary exits non-zero when overall recall < threshold so it slots cleanly
into a CI step.

## Add a case

1. Open `data/kiro-help.jsonl`.
2. Append one JSON object per line with:
   ```json
   {"id":"unique-id","query":"...","must_retrieve_any_of":["docs/foo.md"],"category":"foo"}
   ```
3. Re-run the eval.

Categories you should aim to cover: `auth`, `mcp`, `agents`, `slash-commands`,
`tools`, `errors`, `release-notes`, `config`, `hooks`, `ui`.

## What's tested

- JSONL parsing tolerates blank lines and `//` comments.
- Embedded cases all have `id` / `query` / non-empty `must_retrieve_any_of`.
- `run_eval` passes when any one expected path appears in the top-N retrieved
  set; fails otherwise.
- Recall is broken down per category in the report.

The Bedrock client itself isn't unit tested — that path runs against a live KB
in CI / locally. Tests use `kiro_knowledge_mcp::StubRetriever`.

## See also

- Spec: [docs/superpowers/specs/2026-05-13-kiro-help-bot-design.md](../../docs/superpowers/specs/2026-05-13-kiro-help-bot-design.md) — sec 2 (eval crate)
- Plan: [docs/superpowers/plans/2026-05-19-phase-5-eval-and-release-automation.md](../../docs/superpowers/plans/2026-05-19-phase-5-eval-and-release-automation.md)
- Sister crate: [`kiro-knowledge-mcp`](../kiro-knowledge-mcp/) — provides the
  `Retriever` trait and `BedrockRetriever` impl used here.
