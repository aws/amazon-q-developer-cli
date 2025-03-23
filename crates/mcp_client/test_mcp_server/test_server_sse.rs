//! This is a bin used solely for testing the client with SSE

use mcp_ectors::examples::HelloWorldRouter;
use mcp_ectors::McpServer;
use mcp_ectors::router::RouterServiceManager;
use mcp_ectors::transport::sse_transport_actor::SseTransportConfig;
use mcp_ectors::transport::transport_config::Config;
use tokio::signal;

#[actix::main]
async fn main() {

    let config = SseTransportConfig {
        port: 8080,
        tls_cert: None,
        tls_key: None,
        log_dir: "logs".into(),
        log_file: "sse.log".into(),
    };

    let mut router_manager = RouterServiceManager::default().await;
    let hw_id = "helloworld".to_string();
    let hw_router = Box::new(HelloWorldRouter::new());
    router_manager.register_router::<HelloWorldRouter>(hw_id, hw_router).await.expect("router could not be registered");

    let server = McpServer::new()
        .router_manager(router_manager)
        .transport(Config::Sse(config)) // Fluent API for transport
        .start()
        .unwrap();

    let shutdown_signal = async {
        signal::ctrl_c().await.expect("Failed to listen for ctrl_c signal");
    };

    tokio::select! {
        _ = shutdown_signal => {
            let _ = server.stop();
        }
    }

}
