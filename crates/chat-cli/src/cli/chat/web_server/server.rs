use axum::{routing::get, Router};
use eyre::Result;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::Notify;
use tower_http::cors::{Any, CorsLayer};
use tower_http::services::ServeDir;

use crate::agent_env::session::Session;
use crate::os::Os;

use super::web_ui::WebUI;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub session: Arc<Session>,
    pub web_ui: Arc<WebUI>,
    pub os: Arc<Os>,
}

/// Web server for serving WebUI
pub struct WebServer {
    addr: SocketAddr,
    state: AppState,
}

impl WebServer {
    /// Create new web server
    pub fn new(addr: SocketAddr, session: Arc<Session>, web_ui: Arc<WebUI>, os: Arc<Os>) -> Self {
        Self {
            addr,
            state: AppState { session, web_ui, os },
        }
    }

    /// Build router with all routes
    fn build_router(&self) -> Router {
        Router::new()
            // WebSocket endpoint (global, not per-worker)
            .route("/ws", get(super::websocket::websocket_handler))
            // REST API endpoints
            .route("/api/health", get(super::api::health_check))
            .route("/api/workers", get(super::api::list_workers))
            .route("/api/workers/:id", get(super::api::get_worker))
            // Static file serving (frontend)
            .nest_service("/", ServeDir::new("web/public"))
            // CORS for development
            .layer(CorsLayer::new().allow_origin(Any).allow_methods(Any).allow_headers(Any))
            // Shared state
            .with_state(self.state.clone())
    }

    /// Run the web server (blocks until shutdown)
    pub async fn run(self) -> Result<()> {
        let router = self.build_router();

        tracing::info!("Web server listening on http://{}", self.addr);

        let listener = tokio::net::TcpListener::bind(self.addr).await?;

        axum::serve(listener, router).await?;

        Ok(())
    }

    /// Run with graceful shutdown signal
    pub async fn run_with_shutdown(self, shutdown_signal: Arc<Notify>) -> Result<()> {
        let router = self.build_router();

        let listener = tokio::net::TcpListener::bind(self.addr).await?;
        
        let address = format!("http://{}", self.addr);
        tracing::info!("Web server listening on {}", address);
        
        // Publish ServerStarted event
        self.state.session.event_bus().publish(
            crate::agent_env::AgentEnvironmentEvent::WebUI(
                crate::agent_env::events::WebUIEvent::ServerStarted {
                    address,
                    timestamp: std::time::Instant::now(),
                }
            )
        );

        axum::serve(listener, router)
            .with_graceful_shutdown(async move {
                shutdown_signal.notified().await;
                tracing::info!("Web server shutting down");

                // Give connections 5 seconds to close
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            })
            .await?;

        Ok(())
    }
}
