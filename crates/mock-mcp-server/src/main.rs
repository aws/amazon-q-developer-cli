use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

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

    /// Delay in milliseconds before starting the server (simulates slow startup)
    #[arg(long)]
    startup_delay_ms: Option<u64>,

    /// Keep the process alive after the MCP transport closes (simulates a misbehaving server)
    #[arg(long)]
    linger: bool,

    /// Enable a fully working OAuth flow (discovery, dynamic client registration,
    /// authorize, token, refresh) and require a valid bearer token on `/mcp`.
    ///
    /// This is independent of `--probe-status`: with `--oauth` the server can complete
    /// the OAuth handshake end-to-end, not just trigger it.
    #[arg(long)]
    oauth: bool,

    /// Lifetime (in seconds) of issued OAuth access tokens. Use a small value to
    /// force mid-session token expiry / refresh in tests. Only used with `--oauth`.
    #[arg(long, default_value = "3600")]
    oauth_token_ttl_secs: u64,

    /// Do not issue a refresh token alongside access tokens. This forces the client
    /// down the re-authorization path when the access token expires. Only used with `--oauth`.
    #[arg(long)]
    oauth_no_refresh_token: bool,

    /// Reject `grant_type=refresh_token` requests with HTTP 400. Combined with a short
    /// token TTL this simulates a server that can no longer refresh a session, so the
    /// client must surface a clear error. Only used with `--oauth`.
    #[arg(long)]
    oauth_refresh_fails: bool,
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

async fn run_stdio(server: MockMcpServer, linger: bool) -> Result<()> {
    let service = server.serve(stdio()).await.inspect_err(|e| {
        eprintln!("Serving error: {:?}", e);
    })?;
    service.waiting().await?;
    if linger {
        // Simulate a misbehaving server that doesn't exit when stdin closes
        loop {
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    }
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

/// Configuration for the OAuth-enabled mock server.
#[derive(Clone, Copy)]
struct OAuthMockConfig {
    /// Lifetime of issued access tokens, in seconds.
    token_ttl_secs: u64,
    /// Whether to include a `refresh_token` in token responses.
    issue_refresh_token: bool,
    /// Whether `grant_type=refresh_token` requests should fail with HTTP 400.
    refresh_fails: bool,
}

/// Shared state for the OAuth mock: a registry of currently-valid access tokens
/// mapped to their expiry instants, plus a monotonic counter for unique values.
#[derive(Clone)]
struct OAuthRuntime {
    port: u16,
    cfg: OAuthMockConfig,
    /// access_token -> expiry instant
    tokens: std::sync::Arc<std::sync::Mutex<HashMap<String, std::time::Instant>>>,
    counter: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl OAuthRuntime {
    fn next_id(&self) -> u64 {
        self.counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }

    /// Mint a fresh access token (and optional refresh token), recording the
    /// access token's expiry so `/mcp` can validate it.
    fn issue_token(&self) -> serde_json::Value {
        let id = self.next_id();
        let access_token = format!("mock-access-token-{id}");
        let expiry = std::time::Instant::now() + Duration::from_secs(self.cfg.token_ttl_secs);
        self.tokens.lock().unwrap().insert(access_token.clone(), expiry);

        let mut body = serde_json::json!({
            "access_token": access_token,
            "token_type": "bearer",
            "expires_in": self.cfg.token_ttl_secs,
            "scope": "openid email profile"
        });
        if self.cfg.issue_refresh_token {
            body["refresh_token"] = serde_json::Value::String(format!("mock-refresh-token-{id}"));
        }
        body
    }

    /// Returns true if the bearer token in the request is known and unexpired.
    fn is_token_valid(&self, token: &str) -> bool {
        self.tokens
            .lock()
            .unwrap()
            .get(token)
            .is_some_and(|expiry| *expiry > std::time::Instant::now())
    }
}

/// Run the mock MCP server over HTTP with a fully working OAuth flow.
///
/// Unlike `run_http` with `--probe-status` (which only *triggers* OAuth by
/// returning a 401), this variant implements every endpoint the rmcp client
/// needs to complete the handshake and refresh tokens:
///
/// - `GET /.well-known/oauth-protected-resource` — points at this server as the auth server
/// - `GET /.well-known/oauth-authorization-server` — advertises the endpoints below
/// - `POST /oauth/register` — dynamic client registration (returns a client_id)
/// - `GET  /oauth/authorize` — auto-approves and 302-redirects to the client's loopback with `code`
///   + `state`
/// - `POST /oauth/token` — exchanges auth codes and refresh tokens for access tokens
///
/// Requests to `/mcp` must carry a valid, unexpired bearer token; otherwise the
/// server responds 401 with a `WWW-Authenticate` header so the client kicks off
/// (or retries) the OAuth flow.
async fn run_http_oauth(server: MockMcpServer, port: u16, cfg: OAuthMockConfig) -> Result<()> {
    use axum::extract::{
        Form,
        Query,
    };
    use axum::http::{
        StatusCode,
        header,
    };
    use axum::response::{
        IntoResponse,
        Json,
    };
    use axum::routing::{
        get,
        post,
    };

    let runtime = OAuthRuntime {
        port,
        cfg,
        tokens: std::sync::Arc::new(std::sync::Mutex::new(HashMap::new())),
        counter: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
    };

    let mcp_service = StreamableHttpService::new(
        move || Ok(server.clone()),
        LocalSessionManager::default().into(),
        Default::default(),
    );

    // --- OAuth metadata discovery (RFC 8414 / RFC 9728) ---
    let authorization_metadata = move |port: u16| {
        serde_json::json!({
            "issuer": format!("http://127.0.0.1:{port}"),
            "authorization_endpoint": format!("http://127.0.0.1:{port}/oauth/authorize"),
            "token_endpoint": format!("http://127.0.0.1:{port}/oauth/token"),
            "registration_endpoint": format!("http://127.0.0.1:{port}/oauth/register"),
            "response_types_supported": ["code"],
            "grant_types_supported": ["authorization_code", "refresh_token"],
            "code_challenge_methods_supported": ["S256"],
            "scopes_supported": ["openid", "email", "profile", "offline_access"],
            "token_endpoint_auth_methods_supported": ["none"]
        })
    };
    let protected_resource_metadata = move |port: u16| {
        serde_json::json!({
            "resource": format!("http://127.0.0.1:{port}/mcp"),
            "authorization_servers": [format!("http://127.0.0.1:{port}")]
        })
    };

    let auth_meta_handler = {
        move || {
            let body = authorization_metadata(port);
            async move { Json(body) }
        }
    };
    let resource_meta_handler = {
        move || {
            let body = protected_resource_metadata(port);
            async move { Json(body) }
        }
    };

    // --- Dynamic client registration ---
    let register_handler = move |_body: Option<Json<serde_json::Value>>| async move {
        Json(serde_json::json!({
            "client_id": "mock-oauth-client",
            "client_name": "mock-oauth-client",
            "redirect_uris": [],
            "grant_types": ["authorization_code", "refresh_token"],
            "token_endpoint_auth_method": "none",
            "response_types": ["code"]
        }))
    };

    // --- Authorization endpoint: auto-approve, redirect back to the client loopback ---
    let authorize_runtime = runtime.clone();
    let authorize_handler = move |Query(params): Query<HashMap<String, String>>| {
        let runtime = authorize_runtime.clone();
        async move {
            let Some(redirect_uri) = params.get("redirect_uri") else {
                return (StatusCode::BAD_REQUEST, "missing redirect_uri").into_response();
            };
            // Echo the client's CSRF state back unchanged so the token exchange
            // can correlate the PKCE verifier the client stored.
            let state = params.get("state").cloned().unwrap_or_default();
            let code = format!("mock-auth-code-{}", runtime.next_id());
            let separator = if redirect_uri.contains('?') { '&' } else { '?' };
            let location = format!("{redirect_uri}{separator}code={code}&state={state}");
            (StatusCode::FOUND, [(header::LOCATION, location)]).into_response()
        }
    };

    // --- Token endpoint: authorization_code + refresh_token grants ---
    let token_runtime = runtime.clone();
    let token_handler = move |Form(form): Form<HashMap<String, String>>| {
        let runtime = token_runtime.clone();
        async move {
            let grant_type = form.get("grant_type").map(String::as_str).unwrap_or("");
            if grant_type == "refresh_token" && runtime.cfg.refresh_fails {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(serde_json::json!({
                        "error": "invalid_grant",
                        "error_description": "mock server configured to reject token refresh"
                    })),
                )
                    .into_response();
            }
            Json(runtime.issue_token()).into_response()
        }
    };

    // --- Test control: invalidate all currently-issued access tokens. ---
    // Lets a test simulate server-side token expiry mid-session without relying
    // on wall-clock timing: after this, the client's bearer token is rejected
    // (401) on the next `/mcp` request, forcing a refresh or re-authorization.
    let expire_runtime = runtime.clone();
    let expire_handler = move || {
        let runtime = expire_runtime.clone();
        async move {
            runtime.tokens.lock().unwrap().clear();
            StatusCode::NO_CONTENT
        }
    };

    // --- Bearer-token validation for /mcp ---
    let mcp_runtime = runtime.clone();
    let mcp_auth_layer =
        axum::middleware::from_fn(move |request: axum::extract::Request, next: axum::middleware::Next| {
            let runtime = mcp_runtime.clone();
            async move {
                let authorized = request
                    .headers()
                    .get(header::AUTHORIZATION)
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.strip_prefix("Bearer "))
                    .is_some_and(|token| runtime.is_token_valid(token));

                if authorized {
                    next.run(request).await
                } else {
                    let www_authenticate = format!(
                        "Bearer resource_metadata=\"http://127.0.0.1:{}/.well-known/oauth-protected-resource\"",
                        runtime.port
                    );
                    (StatusCode::UNAUTHORIZED, [(header::WWW_AUTHENTICATE, www_authenticate)]).into_response()
                }
            }
        });

    let mcp_router = axum::Router::new().fallback_service(mcp_service).layer(mcp_auth_layer);

    let router = axum::Router::new()
        .route("/.well-known/oauth-authorization-server", get(auth_meta_handler))
        .route("/mcp/.well-known/oauth-authorization-server", get(auth_meta_handler))
        .route("/.well-known/oauth-protected-resource", get(resource_meta_handler))
        .route("/mcp/.well-known/oauth-protected-resource", get(resource_meta_handler))
        .route("/oauth/register", post(register_handler))
        .route("/oauth/authorize", get(authorize_handler))
        .route("/oauth/token", post(token_handler))
        .route("/control/expire-tokens", post(expire_handler))
        .nest("/mcp", mcp_router);

    let addr = format!("0.0.0.0:{port}");
    eprintln!("Starting OAuth-enabled HTTP MCP server on {addr}");

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

    if let Some(delay_ms) = args.startup_delay_ms {
        tokio::time::sleep(Duration::from_millis(delay_ms)).await;
    }

    let server = MockMcpServer::from_config(&args.config)?;

    match args.transport {
        Transport::Stdio => run_stdio(server, args.linger).await,
        Transport::Http if args.oauth => {
            run_http_oauth(server, args.port, OAuthMockConfig {
                token_ttl_secs: args.oauth_token_ttl_secs,
                issue_refresh_token: !args.oauth_no_refresh_token,
                refresh_fails: args.oauth_refresh_fails,
            })
            .await
        },
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
