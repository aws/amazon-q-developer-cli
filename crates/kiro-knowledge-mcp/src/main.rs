//! kiro-knowledge-mcp — MCP stdio server exposing search_kiro_knowledge,
//! backed either by Amazon Bedrock or by an in-memory stub for testing.

use std::sync::Arc;

use anyhow::Result;
use clap::Parser;
use kiro_knowledge_mcp::retrieve::BedrockRetriever;
use kiro_knowledge_mcp::{
    RetrievedChunk,
    Retriever,
    SearchInput,
    StubRetriever,
    format_chunks,
};
use rmcp::model::*;
use rmcp::service::{
    RequestContext,
    RoleServer,
};
use rmcp::transport::stdio;
use rmcp::{
    ServerHandler,
    ServiceExt,
};

#[derive(Parser, Debug)]
#[command(name = "kiro-knowledge-mcp", version, about)]
struct Args {
    /// Bedrock Knowledge Base ID. Required unless --stub is set.
    #[arg(long, env = "KIRO_KNOWLEDGE_KB_ID")]
    kb_id: Option<String>,

    /// AWS region. Falls back to the default credential chain if unset.
    #[arg(long, env = "AWS_REGION")]
    region: Option<String>,

    /// Use an in-memory stub retriever instead of Bedrock. For testing only.
    #[arg(long)]
    stub: bool,
}

#[derive(Clone)]
struct KnowledgeServer {
    retriever: Arc<dyn Retriever>,
}

impl ServerHandler for KnowledgeServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            instructions: Some("Search the kiro-cli documentation, GitHub issues, and release notes.".to_string()),
            ..Default::default()
        }
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let schema: serde_json::Value = serde_json::json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "source_filter": {
                    "type": "string",
                    "enum": ["docs", "issues", "releases", "all"],
                    "default": "all"
                },
                "max_results": { "type": "integer", "default": 5 }
            },
            "required": ["query"]
        });
        let tool = Tool {
            name: "search_kiro_knowledge".to_string().into(),
            description: Some(
                "Search the kiro-cli documentation, GitHub issues, and release notes for relevant context. Use this whenever the user asks about kiro-cli behavior, errors, or how-tos before answering.".to_string().into()
            ),
            input_schema: Arc::new(serde_json::from_value(schema).unwrap_or_default()),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            title: None,
            meta: None,
        };
        Ok(ListToolsResult::with_all_items(vec![tool]))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        if request.name.as_ref() != "search_kiro_knowledge" {
            return Err(ErrorData::invalid_params(
                format!("unknown tool: {}", request.name),
                None,
            ));
        }

        let args_value: serde_json::Value = match request.arguments {
            Some(map) => serde_json::Value::Object(map.into_iter().collect()),
            None => serde_json::Value::Object(Default::default()),
        };

        let input: SearchInput = serde_json::from_value(args_value)
            .map_err(|e| ErrorData::invalid_params(format!("invalid arguments: {e}"), None))?;

        let chunks: Vec<RetrievedChunk> = self
            .retriever
            .retrieve(&input)
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let text = format_chunks(&chunks);

        Ok(CallToolResult {
            content: vec![Content::text(text)],
            structured_content: None,
            is_error: Some(false),
            meta: None,
        })
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();

    let args = Args::parse();

    let retriever: Arc<dyn Retriever> = if args.stub {
        Arc::new(StubRetriever::new(vec![RetrievedChunk {
            source_path: "docs/stub.md".into(),
            content: "Stub response from kiro-knowledge-mcp.".into(),
            relevance: 0.42,
        }]))
    } else {
        let kb_id = args.kb_id.ok_or_else(|| {
            anyhow::anyhow!("--kb-id is required when --stub is not set (or set KIRO_KNOWLEDGE_KB_ID)")
        })?;
        Arc::new(BedrockRetriever::new(kb_id).await?)
    };

    let server = KnowledgeServer { retriever };
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
