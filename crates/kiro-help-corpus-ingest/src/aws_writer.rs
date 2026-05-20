//! Live `CorpusWriter` impl backed by AWS SDKs (S3 + Bedrock Agent control
//! plane). The trait surface itself lives in [`crate::runner`]; this file
//! is the production wiring and intentionally has no unit tests — its
//! correctness comes from running it against real AWS in CI / integration.

use anyhow::Context;
use aws_sdk_bedrockagent::Client as BedrockAgentClient;
use aws_sdk_s3::Client as S3Client;
use aws_sdk_s3::primitives::ByteStream;

use crate::manifest::Manifest;
use crate::normalize::Batch;
use crate::runner::CorpusWriter;

pub struct AwsCorpusWriter {
    s3: S3Client,
    bedrock_agent: BedrockAgentClient,
    bucket: String,
    knowledge_base_id: String,
    data_source_id: String,
}

impl AwsCorpusWriter {
    pub async fn from_env(
        bucket: impl Into<String>,
        knowledge_base_id: impl Into<String>,
        data_source_id: impl Into<String>,
        region: Option<String>,
    ) -> anyhow::Result<Self> {
        let mut loader = aws_config::defaults(aws_config::BehaviorVersion::latest());
        if let Some(r) = region {
            loader = loader.region(aws_config::Region::new(r));
        }
        let cfg = loader.load().await;
        Ok(Self {
            s3: S3Client::new(&cfg),
            bedrock_agent: BedrockAgentClient::new(&cfg),
            bucket: bucket.into(),
            knowledge_base_id: knowledge_base_id.into(),
            data_source_id: data_source_id.into(),
        })
    }
}

#[async_trait::async_trait]
impl CorpusWriter for AwsCorpusWriter {
    async fn put_batch(&self, batch: &Batch) -> anyhow::Result<()> {
        self.s3
            .put_object()
            .bucket(&self.bucket)
            .key(&batch.key)
            .content_type("application/x-ndjson")
            .body(ByteStream::from(batch.body.clone()))
            .send()
            .await
            .with_context(|| format!("PutObject s3://{}/{}", self.bucket, batch.key))?;
        Ok(())
    }

    async fn put_manifest(&self, manifest: &Manifest) -> anyhow::Result<()> {
        self.s3
            .put_object()
            .bucket(&self.bucket)
            .key("manifest.json")
            .content_type("application/json")
            .body(ByteStream::from(manifest.to_json()))
            .send()
            .await
            .with_context(|| format!("PutObject s3://{}/manifest.json", self.bucket))?;
        Ok(())
    }

    async fn start_ingestion(&self) -> anyhow::Result<String> {
        let resp = self
            .bedrock_agent
            .start_ingestion_job()
            .knowledge_base_id(&self.knowledge_base_id)
            .data_source_id(&self.data_source_id)
            .send()
            .await
            .context("StartIngestionJob")?;
        let job = resp
            .ingestion_job
            .ok_or_else(|| anyhow::anyhow!("StartIngestionJob returned no ingestionJob"))?;
        Ok(job.ingestion_job_id)
    }
}

/// Resolve the first data source on a knowledge base. Helpers like this
/// belong here rather than in main.rs so the binary stays a thin shell.
pub async fn resolve_data_source_id(
    bedrock_agent: &BedrockAgentClient,
    knowledge_base_id: &str,
) -> anyhow::Result<String> {
    let resp = bedrock_agent
        .list_data_sources()
        .knowledge_base_id(knowledge_base_id)
        .send()
        .await
        .context("ListDataSources")?;
    let summaries = resp.data_source_summaries;
    let first = summaries
        .first()
        .ok_or_else(|| anyhow::anyhow!("knowledge base {knowledge_base_id} has no data sources"))?;
    Ok(first.data_source_id.clone())
}
