# kiro-help-corpus-ingest

Lambda binary that runs hourly to keep the kiro-help Bedrock Knowledge Base in
sync with the kiro-cli repo.

It runs every registered `Source`, normalizes raw chunks into JSONL batches, and
writes them to the corpus S3 bucket at:

```
<partition>/<run_iso>/batch-<n>.jsonl
```

Then it calls `bedrock-agent:StartIngestionJob` to refresh the KB.

## Status

Phase 3 v0 (this commit): testable core only.

- ✅ `Source` trait + `RawChunk` + `Partition` types
- ✅ JSONL batch normalizer with size-based rollover
- ✅ Stub source for tests
- ⏳ Live `git_source` / `github_source` / `release_source`
- ⏳ S3 writer + `StartIngestionJob` plumbing in `main.rs`
- ⏳ `lambda_runtime::run` wrapper

The skeleton ships because the trait surface is what other phases depend on.
Live wiring is the next commit on this crate.

## Build

```bash
cargo test -p kiro-help-corpus-ingest

# Lambda artefact (ARM64 — match the runtime stack):
cargo lambda build --release --arm64 -p kiro-help-corpus-ingest
# bootstrap binary lives at:
#   target/lambda/kiro-help-corpus-ingest/bootstrap
```

`cargo-lambda` is required for the Lambda artefact path. The library's unit
tests do not need it.

## Wiring (Kiro-botCDK)

The `KiroBotIngestStack` in `Kiro-botCDK` consumes the `bootstrap` binary as a
Lambda asset and pins runtime to `provided.al2023` on `arm64`. EventBridge fires
the function on a top-of-hour schedule.

## See also

- Spec: [docs/superpowers/specs/2026-05-13-kiro-help-bot-design.md](../../docs/superpowers/specs/2026-05-13-kiro-help-bot-design.md)
- Phase 3 plan: [docs/superpowers/plans/2026-05-19-phase-3-ingest-pipeline.md](../../docs/superpowers/plans/2026-05-19-phase-3-ingest-pipeline.md)
