use anyhow::Result;
use code_agent_sdk::mcp::CodeIntelligenceServer;
use rmcp::{transport::stdio, ServiceExt};

#[tokio::main]
async fn main() -> Result<()> {

    // Create and serve the server via stdio
    let service = CodeIntelligenceServer::new()
        .serve(stdio())
        .await
        .inspect_err(|e| {
            tracing::error!("Serving error: {:?}", e);
        })?;

    service.waiting().await?;
    Ok(())
}
