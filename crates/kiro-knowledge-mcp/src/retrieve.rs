//! Production `Retriever` impl backed by Amazon Bedrock Knowledge Base.

use anyhow::{
    Context,
    Result,
};
use aws_sdk_bedrockagentruntime::Client;
use aws_sdk_bedrockagentruntime::types::{
    KnowledgeBaseQuery,
    KnowledgeBaseRetrievalConfiguration,
    KnowledgeBaseVectorSearchConfiguration,
};

use crate::{
    RetrievedChunk,
    Retriever,
    SearchInput,
};

pub struct BedrockRetriever {
    client: Client,
    knowledge_base_id: String,
}

impl BedrockRetriever {
    pub async fn new(knowledge_base_id: impl Into<String>) -> Result<Self> {
        let config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
        let client = Client::new(&config);
        Ok(Self {
            client,
            knowledge_base_id: knowledge_base_id.into(),
        })
    }
}

#[async_trait::async_trait]
impl Retriever for BedrockRetriever {
    async fn retrieve(&self, input: &SearchInput) -> Result<Vec<RetrievedChunk>> {
        // NOTE: source_filter is intentionally ignored in the v1 cut.
        // The agent can filter post-hoc using `source_path` on the returned
        // chunks. Wiring up Bedrock RetrievalFilter::Equals against the
        // metadata attribute "source" is a Phase 1.5 follow-up — see TODO
        // below.
        let _ = input.source_filter; // silence unused warning until 1.5

        // SDK builders return the value directly (not Result). Validation
        // happens at request-send time.
        let query = KnowledgeBaseQuery::builder().text(&input.query).build();

        let vector_cfg = KnowledgeBaseVectorSearchConfiguration::builder()
            .number_of_results(input.max_results as i32)
            .build();

        let retrieval_cfg = KnowledgeBaseRetrievalConfiguration::builder()
            .vector_search_configuration(vector_cfg)
            .build();

        let resp = self
            .client
            .retrieve()
            .knowledge_base_id(&self.knowledge_base_id)
            .retrieval_query(query)
            .retrieval_configuration(retrieval_cfg)
            .send()
            .await
            .context("calling Bedrock Retrieve")?;

        let chunks = resp
            .retrieval_results
            .into_iter()
            .map(|result| {
                // RetrievalResultContent.text is a non-Option String.
                let content = result.content.as_ref().map(|c| c.text.clone()).unwrap_or_default();
                let source_path = result
                    .location
                    .as_ref()
                    .and_then(|loc| loc.s3_location.as_ref())
                    .and_then(|s3| s3.uri.clone())
                    .unwrap_or_else(|| "<unknown>".to_string());
                let relevance = result.score.unwrap_or(0.0);
                RetrievedChunk {
                    source_path,
                    content,
                    relevance,
                }
            })
            .collect();

        Ok(chunks)
    }
}

// TODO(phase-1.5): wire up source_filter via RetrievalFilter::Equals on a
// `source` metadata attribute. The exact builder shape depends on the
// pinned SDK version — when implementing, run `cargo doc --open
// -p aws-sdk-bedrockagentruntime` and look at `RetrievalFilter` and
// `FilterAttribute`. Reference: the ingest Lambda (Phase 5) writes a
// `source` field on every chunk it stores in S3, so the filter target
// already exists in the data.
