//! Conversion utilities for ACP MCP server types to agent MCP server config types.

use agent::agent_config::definitions::{
    LocalMcpServerConfig,
    McpServerConfig,
    RemoteMcpServerConfig,
    default_timeout,
};
use sacp::schema::{
    McpServer,
    McpServerHttp,
    McpServerSse,
    McpServerStdio,
};

#[derive(Debug, thiserror::Error)]
pub enum ConversionError {
    #[error("Unknown MCP server transport type")]
    UnknownTransport,
}

pub fn convert_mcp_server(acp_server: McpServer) -> Result<(String, McpServerConfig), ConversionError> {
    match acp_server {
        McpServer::Stdio(stdio) => {
            let name = stdio.name.clone();
            Ok((name, McpServerConfig::Local(convert_stdio(stdio))))
        },
        McpServer::Http(http) => {
            let name = http.name.clone();
            Ok((name, McpServerConfig::Remote(convert_http(http))))
        },
        McpServer::Sse(sse) => {
            let name = sse.name.clone();
            Ok((name, McpServerConfig::Remote(convert_sse(sse))))
        },
        _ => Err(ConversionError::UnknownTransport),
    }
}

pub fn convert_stdio(stdio: McpServerStdio) -> LocalMcpServerConfig {
    let env = if stdio.env.is_empty() {
        None
    } else {
        Some(stdio.env.into_iter().map(|e| (e.name, e.value)).collect())
    };

    LocalMcpServerConfig {
        command: stdio.command.display().to_string(),
        args: stdio.args,
        env,
        timeout_ms: default_timeout(),
        disabled: false,
    }
}

pub fn convert_http(http: McpServerHttp) -> RemoteMcpServerConfig {
    RemoteMcpServerConfig {
        url: http.url,
        headers: http.headers.into_iter().map(|h| (h.name, h.value)).collect(),
        timeout_ms: default_timeout(),
        oauth_scopes: Vec::new(),
        oauth: None,
        disabled: false,
    }
}

pub fn convert_sse(sse: McpServerSse) -> RemoteMcpServerConfig {
    RemoteMcpServerConfig {
        url: sse.url,
        headers: sse.headers.into_iter().map(|h| (h.name, h.value)).collect(),
        timeout_ms: default_timeout(),
        oauth_scopes: Vec::new(),
        oauth: None,
        disabled: false,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use sacp::schema::{
        EnvVariable,
        HttpHeader,
    };

    use super::*;

    #[test]
    fn test_stdio_minimal() {
        let stdio = McpServerStdio::new("test-server", PathBuf::from("/usr/bin/test")).args(vec!["arg1".to_string()]);

        let config = convert_stdio(stdio);
        assert_eq!(config.command, "/usr/bin/test");
        assert_eq!(config.args, vec!["arg1"]);
        assert_eq!(config.env, None);
        assert_eq!(config.timeout_ms, 120_000);
        assert!(!config.disabled);
    }

    #[test]
    fn test_stdio_with_env() {
        let stdio = McpServerStdio::new("test-server", PathBuf::from("/usr/bin/test")).env(vec![
            EnvVariable::new("KEY1", "value1"),
            EnvVariable::new("KEY2", "value2"),
        ]);

        let config = convert_stdio(stdio);
        let env = config.env.unwrap();
        assert_eq!(env.get("KEY1"), Some(&"value1".to_string()));
        assert_eq!(env.get("KEY2"), Some(&"value2".to_string()));
    }

    #[test]
    fn test_http_minimal() {
        let http = McpServerHttp::new("test-server", "http://localhost:8080");

        let config = convert_http(http);
        assert_eq!(config.url, "http://localhost:8080");
        assert!(config.headers.is_empty());
        assert_eq!(config.timeout_ms, 120_000);
        assert!(config.oauth_scopes.is_empty());
        assert!(config.oauth.is_none());
        assert!(!config.disabled);
    }

    #[test]
    fn test_http_with_headers() {
        let http = McpServerHttp::new("test-server", "http://localhost:8080").headers(vec![
            HttpHeader::new("Authorization", "Bearer token"),
            HttpHeader::new("X-Custom", "custom-value"),
        ]);

        let config = convert_http(http);
        assert_eq!(config.headers.get("Authorization"), Some(&"Bearer token".to_string()));
        assert_eq!(config.headers.get("X-Custom"), Some(&"custom-value".to_string()));
    }

    #[test]
    fn test_sse_supported() {
        let sse = sacp::schema::McpServerSse::new("test-server", "http://localhost:8080");

        let config = convert_sse(sse);
        assert_eq!(config.url, "http://localhost:8080");
        assert!(config.headers.is_empty());
        assert_eq!(config.timeout_ms, 120_000);
    }

    #[test]
    fn test_convert_mcp_server_sse() {
        let sse = sacp::schema::McpServerSse::new("test-server", "http://localhost:8080");

        let (name, config) = convert_mcp_server(McpServer::Sse(sse)).unwrap();
        assert_eq!(name, "test-server");
        assert!(matches!(config, McpServerConfig::Remote(_)));
    }

    #[test]
    fn test_convert_mcp_server_stdio() {
        let stdio = McpServerStdio::new("test-server", PathBuf::from("/usr/bin/test"));

        let (name, config) = convert_mcp_server(McpServer::Stdio(stdio)).unwrap();
        assert_eq!(name, "test-server");
        assert!(matches!(config, McpServerConfig::Local(_)));
    }

    #[test]
    fn test_convert_mcp_server_http() {
        let http = McpServerHttp::new("test-server", "http://localhost:8080");

        let (name, config) = convert_mcp_server(McpServer::Http(http)).unwrap();
        assert_eq!(name, "test-server");
        assert!(matches!(config, McpServerConfig::Remote(_)));
    }
}
