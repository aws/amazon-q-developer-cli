use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use clap::{
    Parser,
    ValueEnum,
};
use mock_mcp_server::{
    MockResponse,
    ToolDef,
    find_response,
    parse_config,
};
use rmcp::model::*;
use rmcp::service::{
    RequestContext,
    RoleServer,
};
use rmcp::transport::stdio;
use rmcp::transport::streamable_http_server::StreamableHttpService;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::{
    ServerHandler,
    ServiceExt,
};

#[derive(Debug, Clone, ValueEnum)]
enum Transport {
    Stdio,
    Http,
}

#[derive(Parser)]
#[command(name = "mock-mcp-server")]
#[command(about = "Mock MCP server for testing")]
struct Args {
    /// Path to JSONL file containing tool definitions and responses
    #[arg(long, short)]
    config: PathBuf,

    /// Transport type
    #[arg(long, short, default_value = "stdio")]
    transport: Transport,

    /// Port for HTTP transport
    #[arg(long, short, default_value = "8080")]
    port: u16,

    /// HTTP status code to return for probe requests (e.g., 401 or 403 to trigger OAuth)
    #[arg(long)]
    probe_status: Option<u16>,
}

#[derive(Clone)]
pub struct MockMcpServer {
    tools: Arc<Vec<ToolDef>>,
    responses: Arc<HashMap<String, Vec<MockResponse>>>,
}

impl MockMcpServer {
    pub fn from_config(path: &PathBuf) -> Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let (tools, responses) = parse_config(&content)?;
        Ok(Self {
            tools: Arc::new(tools),
            responses: Arc::new(responses),
        })
    }
}

impl ServerHandler for MockMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            instructions: Some("Mock MCP server for testing".to_string()),
            ..Default::default()
        }
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let tools = self
            .tools
            .iter()
            .map(|t| Tool {
                name: t.name.clone().into(),
                description: Some(t.description.clone().into()),
                input_schema: Arc::new(serde_json::from_value(t.input_schema.clone()).unwrap_or_default()),
                output_schema: None,
                annotations: None,
                execution: None,
                icons: None,
                title: None,
                meta: None,
            })
            .collect();

        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, ErrorData> {
        let tool_name = request.name.as_ref();

        if let Some(response) = find_response(&self.responses, tool_name, &request.arguments) {
            let text = serde_json::to_string_pretty(&response).unwrap_or_default();
            Ok(CallToolResult::success(vec![Content::text(text)]))
        } else {
            Err(ErrorData::new(
                ErrorCode::METHOD_NOT_FOUND,
                format!("No mock response configured for tool: {}", tool_name),
                None,
            ))
        }
    }
}

async fn run_stdio(server: MockMcpServer) -> Result<()> {
    let service = server.serve(stdio()).await.inspect_err(|e| {
        eprintln!("Serving error: {:?}", e);
    })?;
    service.waiting().await?;
    Ok(())
}

async fn run_http(server: MockMcpServer, port: u16, probe_status: Option<u16>) -> Result<()> {
    use std::sync::atomic::{
        AtomicBool,
        Ordering,
    };

    use axum::body::Body;
    use axum::extract::State;
    use axum::http::{
        Request,
        StatusCode,
    };
    use axum::middleware::{
        self,
        Next,
    };
    use axum::response::{
        IntoResponse,
        Json,
        Response,
    };
    use axum::routing::get;

    let service = StreamableHttpService::new(
        move || Ok(server.clone()),
        LocalSessionManager::default().into(),
        Default::default(),
    );

    // OAuth discovery endpoint handler - returns mock OAuth metadata
    async fn oauth_discovery(axum::extract::State(port): axum::extract::State<u16>) -> Json<serde_json::Value> {
        Json(serde_json::json!({
            "issuer": format!("http://127.0.0.1:{}", port),
            "authorization_endpoint": format!("http://127.0.0.1:{}/oauth/authorize", port),
            "token_endpoint": format!("http://127.0.0.1:{}/oauth/token", port),
            "registration_endpoint": format!("http://127.0.0.1:{}/oauth/register", port),
            "response_types_supported": ["code"],
            "scopes_supported": ["openid", "profile"]
        }))
    }

    let router = if let Some(status_code) = probe_status {
        // Track if we've already returned the probe response
        let probe_returned = Arc::new(AtomicBool::new(false));

        async fn probe_middleware(
            State((status_code, probe_returned, port)): State<(u16, Arc<AtomicBool>, u16)>,
            request: Request<Body>,
            next: Next,
        ) -> Response {
            // Only intercept the first POST request to /mcp (the probe)
            if request.method() == axum::http::Method::POST
                && request.uri().path().starts_with("/mcp")
                && !probe_returned.swap(true, Ordering::SeqCst)
            {
                // Return 401 with WWW-Authenticate header for OAuth discovery
                let status = StatusCode::from_u16(status_code).unwrap_or(StatusCode::UNAUTHORIZED);
                return (status, [(
                    axum::http::header::WWW_AUTHENTICATE,
                    format!(
                        "Bearer resource_metadata=\"http://127.0.0.1:{}/.well-known/oauth-protected-resource\"",
                        port
                    ),
                )])
                    .into_response();
            }
            next.run(request).await
        }

        // Protected resource metadata endpoint - points to the authorization server
        async fn oauth_protected_resource(
            axum::extract::State(port): axum::extract::State<u16>,
        ) -> Json<serde_json::Value> {
            Json(serde_json::json!({
                "resource": format!("http://127.0.0.1:{}/mcp", port),
                "authorization_servers": [format!("http://127.0.0.1:{}", port)]
            }))
        }

        axum::Router::new()
            .route("/.well-known/oauth-authorization-server", get(oauth_discovery))
            .route("/mcp/.well-known/oauth-authorization-server", get(oauth_discovery))
            .route("/.well-known/oauth-protected-resource", get(oauth_protected_resource))
            .route(
                "/mcp/.well-known/oauth-protected-resource",
                get(oauth_protected_resource),
            )
            .nest_service("/mcp", service)
            .layer(middleware::from_fn_with_state(
                (status_code, probe_returned, port),
                probe_middleware,
            ))
            .with_state(port)
    } else {
        axum::Router::new()
            .route("/.well-known/oauth-authorization-server", get(oauth_discovery))
            .route("/mcp/.well-known/oauth-authorization-server", get(oauth_discovery))
            .nest_service("/mcp", service)
            .with_state(port)
    };

    let addr = format!("0.0.0.0:{}", port);
    eprintln!("Starting HTTP MCP server on {}", addr);

    let tcp_listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(tcp_listener, router)
        .with_graceful_shutdown(async {
            tokio::signal::ctrl_c().await.ok();
        })
        .await?;

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let server = MockMcpServer::from_config(&args.config)?;

    match args.transport {
        Transport::Stdio => run_stdio(server).await,
        Transport::Http => run_http(server, args.port, args.probe_status).await,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;

    #[test]
    fn test_parse_config() {
        let content = r#"
{"type": "tool", "name": "echo", "description": "Echoes back the input", "input_schema": {"type": "object"}}
{"type": "response", "tool": "echo", "response": {"echoed": "hello"}}
"#;
        let (tools, responses) = parse_config(content).unwrap();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name, "echo");
        assert!(responses.contains_key("echo"));
    }

    #[test]
    fn test_find_response() {
        let content = r#"
{"type": "tool", "name": "add", "description": "Adds numbers"}
{"type": "response", "tool": "add", "response": {"result": 42}}
"#;
        let (_, responses) = parse_config(content).unwrap();
        let response = find_response(&responses, "add", &None);
        assert!(response.is_some());
        assert_eq!(response.unwrap()["result"], 42);
    }

    #[test]
    fn test_missing_response() {
        let content = r#"
{"type": "tool", "name": "echo", "description": "Echoes"}
"#;
        let (_, responses) = parse_config(content).unwrap();
        let response = find_response(&responses, "echo", &None);
        assert!(response.is_none());
    }

    #[test]
    fn test_comments_ignored() {
        let content = r#"
// This is a comment
{"type": "tool", "name": "test", "description": "Test tool"}
// Another comment
"#;
        let (tools, _) = parse_config(content).unwrap();
        assert_eq!(tools.len(), 1);
    }

    #[test]
    fn test_input_matching() {
        let content = r#"
{"type": "tool", "name": "greet", "description": "Greets someone"}
{"type": "response", "tool": "greet", "input_match": {"name": "Alice"}, "response": {"greeting": "Hello Alice!"}}
{"type": "response", "tool": "greet", "input_match": {"name": "Bob"}, "response": {"greeting": "Hey Bob!"}}
{"type": "response", "tool": "greet", "response": {"greeting": "Hello stranger!"}}
"#;
        let (_, responses) = parse_config(content).unwrap();

        // Match Alice
        let mut args = serde_json::Map::new();
        args.insert("name".to_string(), Value::String("Alice".to_string()));
        let response = find_response(&responses, "greet", &Some(args));
        assert_eq!(response.unwrap()["greeting"], "Hello Alice!");

        // Match Bob
        let mut args = serde_json::Map::new();
        args.insert("name".to_string(), Value::String("Bob".to_string()));
        let response = find_response(&responses, "greet", &Some(args));
        assert_eq!(response.unwrap()["greeting"], "Hey Bob!");

        // No match - fall back to default (no input_match)
        let mut args = serde_json::Map::new();
        args.insert("name".to_string(), Value::String("Charlie".to_string()));
        let response = find_response(&responses, "greet", &Some(args));
        assert_eq!(response.unwrap()["greeting"], "Hello stranger!");

        // No args - use default
        let response = find_response(&responses, "greet", &None);
        assert_eq!(response.unwrap()["greeting"], "Hello stranger!");
    }
}
