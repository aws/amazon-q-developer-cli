//! kiro-help corpus ingest Lambda entrypoint.
//!
//! Wakes once an hour via EventBridge, runs every configured `Source`, writes
//! the JSONL batches to S3, then triggers a Bedrock KB ingestion job.
//!
//! Configuration comes entirely from env (set by `KiroBotIngestStack`):
//!
//! - `CORPUS_BUCKET`            — S3 corpus bucket
//! - `KB_ID`                    — Bedrock Knowledge Base ID
//! - `AWS_REGION`               — region for both clients
//! - `KIRO_CLI_REPO`            — `owner/repo`, defaults to `kiro-team/kiro-cli`
//! - `KIRO_CLI_LOCAL_CHECKOUT`  — optional; if set, walked for the docs
//!                                partition. Lambda runs typically don't have
//!                                a checkout — Phase 3 v1 sets this only when
//!                                the Lambda is run with a layer carrying the
//!                                docs tarball. See INGEST.md.
//! - `GH_PAT`                   — GitHub personal access token. The runtime
//!                                stack populates this from
//!                                `kiro-bot/github-pat` Secrets Manager.

use std::env;

use anyhow::Context;
use aws_sdk_bedrockagent::Client as BedrockAgentClient;
use kiro_help_corpus_ingest::{
    Partition,
    Source,
    aws_writer::{
        AwsCorpusWriter,
        resolve_data_source_id,
    },
    git_source::GitSource,
    github_http::{
        GithubIssuesSource,
        GithubReleasesSource,
    },
    runner::run_once,
};
use lambda_runtime::{
    Error,
    LambdaEvent,
    service_fn,
};
use tracing::{
    error,
    info,
};

const DEFAULT_REPO: &str = "kiro-team/kiro-cli";

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_target(false)
        .with_ansi(false)
        .without_time()
        .init();
    let func = service_fn(handler);
    lambda_runtime::run(func).await
}

async fn handler(event: LambdaEvent<serde_json::Value>) -> Result<serde_json::Value, Error> {
    let _ = event;
    let bucket = env::var("CORPUS_BUCKET").context("CORPUS_BUCKET env required")?;
    let kb_id = env::var("KB_ID").context("KB_ID env required")?;
    let region = env::var("AWS_REGION").ok();
    let repo = env::var("KIRO_CLI_REPO").unwrap_or_else(|_| DEFAULT_REPO.to_string());
    let pat = env::var("GH_PAT").ok();
    let local_checkout = env::var("KIRO_CLI_LOCAL_CHECKOUT").ok();

    info!(%bucket, %kb_id, %repo, has_pat = pat.is_some(), "ingest run starting");

    // Resolve the KB's data source ID up front so the writer can carry it.
    let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest());
    if let Some(r) = &region {
        loader = loader.region(aws_config::Region::new(r.clone()));
    }
    let cfg = loader.load().await;
    let bedrock_agent = BedrockAgentClient::new(&cfg);
    let data_source_id = resolve_data_source_id(&bedrock_agent, &kb_id).await?;
    info!(%data_source_id, "resolved KB data source");

    let writer =
        AwsCorpusWriter::from_env(bucket.clone(), kb_id.clone(), data_source_id, region.clone())
            .await?;

    let mut sources: Vec<Box<dyn Source>> = Vec::new();
    if let Some(checkout) = local_checkout.as_deref() {
        sources.push(Box::new(GitSource::new(checkout)));
    } else {
        info!(
            "KIRO_CLI_LOCAL_CHECKOUT not set; docs partition will be empty this run"
        );
        // Push a stub so the run records 0 docs without skipping the partition.
        sources.push(Box::new(kiro_help_corpus_ingest::StubSource::new(
            Partition::Docs,
            Vec::new(),
        )));
    }
    sources.push(Box::new(GithubIssuesSource::new(repo.clone(), pat.clone())));
    sources.push(Box::new(GithubReleasesSource::new(repo.clone(), pat.clone())));

    let run_at = chrono::Utc::now();
    match run_once(sources.as_slice(), &writer, run_at).await {
        Ok(summary) => {
            info!(
                ?summary,
                "ingest run complete"
            );
            Ok(serde_json::json!({
                "status": "ok",
                "ingestion_job_id": summary.ingestion_job_id,
                "batches_written": summary.batches_written,
                "counts": summary.counts.iter().map(|(p, n)| (p.as_str().to_string(), *n)).collect::<std::collections::BTreeMap<_,_>>(),
            }))
        }
        Err(e) => {
            error!(?e, "ingest run failed");
            Err(Error::from(format!("ingest failed: {e:#}")))
        }
    }
}
