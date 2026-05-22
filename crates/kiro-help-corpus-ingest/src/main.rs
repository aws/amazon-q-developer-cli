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
//! - `KIRO_CLI_LOCAL_CHECKOUT`  — optional; if set, walked for the docs partition. Lambda runs
//!   typically don't have a checkout — Phase 3 v1 sets this only when the Lambda is run with a
//!   layer carrying the docs tarball. See INGEST.md.
//! - `GH_PAT`                   — GitHub personal access token. The runtime stack populates this
//!   from `kiro-bot/github-pat` Secrets Manager.

use std::env;

use anyhow::Context;
use aws_sdk_bedrockagent::Client as BedrockAgentClient;
use aws_sdk_secretsmanager::Client as SecretsManagerClient;
use kiro_help_corpus_ingest::aws_writer::{
    AwsCorpusWriter,
    resolve_data_source_id,
};
use kiro_help_corpus_ingest::git_source::GitSource;
use kiro_help_corpus_ingest::github_http::{
    GithubIssuesSource,
    GithubReleasesSource,
};
use kiro_help_corpus_ingest::runner::run_once;
use kiro_help_corpus_ingest::{
    Partition,
    Source,
};
use lambda_runtime::{
    Error,
    LambdaEvent,
    service_fn,
};
use tracing::{
    error,
    info,
    warn,
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
    let local_checkout = env::var("KIRO_CLI_LOCAL_CHECKOUT").ok();

    // Resolve the KB's data source ID up front so the writer can carry it.
    let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest());
    if let Some(r) = &region {
        loader = loader.region(aws_config::Region::new(r.clone()));
    }
    let cfg = loader.load().await;
    let bedrock_agent = BedrockAgentClient::new(&cfg);

    // GitHub PAT — env var first (handy for local testing), then Secrets
    // Manager via GH_PAT_SECRET_ARN / GH_PAT_SECRET_JSON_KEY (the production
    // path; CDK passes the ARN, not the value, so PATs never sit in CFN
    // templates).
    let pat = match env::var("GH_PAT").ok() {
        Some(p) if !p.is_empty() => Some(p),
        _ => match env::var("GH_PAT_SECRET_ARN").ok() {
            Some(arn) => {
                fetch_pat_from_secret(&cfg, &arn, &env::var("GH_PAT_SECRET_JSON_KEY").unwrap_or_default()).await?
            },
            None => None,
        },
    };

    info!(%bucket, %kb_id, %repo, has_pat = pat.is_some(), "ingest run starting");

    let data_source_id = resolve_data_source_id(&bedrock_agent, &kb_id).await?;
    info!(%data_source_id, "resolved KB data source");

    let writer = AwsCorpusWriter::from_env(bucket.clone(), kb_id.clone(), data_source_id, region.clone()).await?;

    let mut sources: Vec<Box<dyn Source>> = Vec::new();
    if let Some(checkout) = local_checkout.as_deref() {
        sources.push(Box::new(GitSource::new(checkout)));
    } else {
        info!("KIRO_CLI_LOCAL_CHECKOUT not set; docs partition will be empty this run");
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
            info!(?summary, "ingest run complete");
            Ok(serde_json::json!({
                "status": "ok",
                "ingestion_job_id": summary.ingestion_job_id,
                "batches_written": summary.batches_written,
                "counts": summary.counts.iter().map(|(p, n)| (p.as_str().to_string(), *n)).collect::<std::collections::BTreeMap<_,_>>(),
            }))
        },
        Err(e) => {
            error!(?e, "ingest run failed");
            Err(Error::from(format!("ingest failed: {e:#}")))
        },
    }
}

/// Pull a GitHub PAT from Secrets Manager. The secret stores a JSON object
/// like `{"KIRO_BOT_PAT":"ghp_..."}`; `json_key` selects which field. Returns
/// `Ok(None)` if the secret is empty or the key is missing — the caller
/// downgrades to "unauthenticated GitHub" which is fine for public repos.
async fn fetch_pat_from_secret(
    cfg: &aws_config::SdkConfig,
    arn: &str,
    json_key: &str,
) -> anyhow::Result<Option<String>> {
    let client = SecretsManagerClient::new(cfg);
    let resp = client
        .get_secret_value()
        .secret_id(arn)
        .send()
        .await
        .context("Secrets Manager GetSecretValue")?;
    let Some(secret_str) = resp.secret_string else {
        warn!(arn, "secret has no SecretString; skipping PAT");
        return Ok(None);
    };
    if json_key.is_empty() {
        // Treat the entire SecretString as the PAT.
        return Ok(Some(secret_str));
    }
    let v: serde_json::Value =
        serde_json::from_str(&secret_str).with_context(|| format!("parsing secret JSON from {arn}"))?;
    let pat = v.get(json_key).and_then(|x| x.as_str()).map(String::from);
    if pat.is_none() {
        warn!(arn, json_key, "secret JSON missing the requested key; skipping PAT");
    }
    Ok(pat)
}
