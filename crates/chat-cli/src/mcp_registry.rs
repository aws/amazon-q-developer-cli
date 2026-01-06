use eyre::Result;
use serde::{
    Deserialize,
    Serialize,
};

/// Categorize registry fetch errors for better user messaging
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RegistryErrorType {
    /// Network connectivity issues (DNS, timeout, connection refused, etc.)
    NetworkConnectivity,
    /// Registry data issues (invalid JSON, validation failures, etc.)
    RegistryData,
}

impl RegistryErrorType {
    /// Categorize an error from registry fetch operations
    pub fn from_error(error: &eyre::Error) -> Self {
        let error_str = error.to_string().to_lowercase();

        // Network connectivity issues
        if error_str.contains("connection")
            || error_str.contains("timeout")
            || error_str.contains("dns")
            || error_str.contains("network")
            || error_str.contains("unreachable")
            || error_str.contains("refused")
            || error_str.contains("timed out")
        {
            return Self::NetworkConnectivity;
        }

        if error_str.contains("http")
            && (error_str.contains("404") || error_str.contains("500") || error_str.contains("503"))
        {
            return Self::NetworkConnectivity;
        }

        // Everything else (JSON parsing, validation, etc.) is registry data issues
        Self::RegistryData
    }
}

/// Cache TTL for MCP registry data and profile config (24 hours)
pub const MCP_CACHE_TTL_HOURS: i64 = 24;

/// HTTP header for remote server
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpHeader {
    pub name: String,
    pub value: String,
}

/// Remote server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoteServerConfig {
    /// Type of remote connection: "streamable-http" or "sse"
    #[serde(rename = "type")]
    pub remote_type: String,
    /// Server endpoint URL
    pub url: String,
    /// Optional HTTP headers
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub headers: Vec<HttpHeader>,
}

/// Runtime or package argument
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Argument {
    /// Must be "positional"
    #[serde(rename = "type")]
    pub arg_type: String,
    /// Argument value
    pub value: String,
}

/// Environment variable
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentVariable {
    pub name: String,
    pub value: String,
}

/// Transport configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transport {
    /// Must be "stdio"
    #[serde(rename = "type")]
    pub transport_type: String,
}

/// Local server package configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageConfig {
    /// Registry type: "npm", "pypi", or "oci"
    #[serde(rename = "registryType")]
    pub registry_type: String,
    /// Optional package registry URL
    #[serde(rename = "registryBaseUrl", default, skip_serializing_if = "Option::is_none")]
    pub registry_base_url: Option<String>,
    /// Package identifier (e.g., "@acme/my-server")
    pub identifier: String,
    /// Transport configuration (must be stdio)
    pub transport: Transport,
    /// Optional runtime arguments (e.g., for npx)
    #[serde(rename = "runtimeArguments", default, skip_serializing_if = "Vec::is_empty")]
    pub runtime_arguments: Vec<Argument>,
    /// Optional package arguments
    #[serde(rename = "packageArguments", default, skip_serializing_if = "Vec::is_empty")]
    pub package_arguments: Vec<Argument>,
    /// Optional environment variables
    #[serde(rename = "environmentVariables", default, skip_serializing_if = "Vec::is_empty")]
    pub environment_variables: Vec<EnvironmentVariable>,
}

/// MCP Server definition from the registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpServerDefinition {
    /// Server name/identifier (required, unique)
    pub name: String,
    /// Human-readable server name (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Description of server (required)
    pub description: String,
    /// Semantic version of server (required)
    pub version: String,
    /// Remote server configurations (for remote servers)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remotes: Vec<RemoteServerConfig>,
    /// Package configurations (for local servers)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub packages: Vec<PackageConfig>,
}

/// Wrapper for a server entry in the registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRegistryServerEntry {
    /// The server definition
    pub server: McpServerDefinition,
}

/// Response from the MCP registry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpRegistryResponse {
    /// Array of server entries
    pub servers: Vec<McpRegistryServerEntry>,
}

impl McpRegistryResponse {
    /// Validate the registry response
    pub fn validate(&self) -> Result<()> {
        if self.servers.is_empty() {
            return Err(eyre::eyre!("Registry contains no servers"));
        }

        // Validate each server
        for (idx, entry) in self.servers.iter().enumerate() {
            Self::validate_server(&entry.server, idx)?;
        }

        // Check for duplicate names
        let mut seen_names = std::collections::HashSet::new();
        for entry in &self.servers {
            if !seen_names.insert(&entry.server.name) {
                return Err(eyre::eyre!("Duplicate server name found: {}", entry.server.name));
            }
        }

        Ok(())
    }

    /// Validate a single server definition
    fn validate_server(server: &McpServerDefinition, idx: usize) -> Result<()> {
        // Check required fields
        if server.name.is_empty() {
            return Err(eyre::eyre!("Server at index {} has empty name", idx));
        }
        if server.description.is_empty() {
            return Err(eyre::eyre!("Server '{}' has empty description", server.name));
        }
        if server.version.is_empty() {
            return Err(eyre::eyre!("Server '{}' has empty version", server.name));
        }

        // Server must be either remote or local, not both
        let is_remote = !server.remotes.is_empty();
        let is_local = !server.packages.is_empty();

        if !is_remote && !is_local {
            return Err(eyre::eyre!(
                "Server '{}' must have either 'remotes' or 'packages' defined",
                server.name
            ));
        }
        if is_remote && is_local {
            return Err(eyre::eyre!(
                "Server '{}' cannot have both 'remotes' and 'packages' defined",
                server.name
            ));
        }

        // Validate remote server
        if is_remote {
            if server.remotes.len() != 1 {
                return Err(eyre::eyre!(
                    "Server '{}' must have exactly one remote configuration, found {}",
                    server.name,
                    server.remotes.len()
                ));
            }
            let remote = &server.remotes[0];
            if remote.remote_type != "streamable-http" && remote.remote_type != "sse" {
                return Err(eyre::eyre!(
                    "Server '{}' has invalid remote type '{}', must be 'streamable-http' or 'sse'",
                    server.name,
                    remote.remote_type
                ));
            }
            if remote.url.is_empty() {
                return Err(eyre::eyre!("Server '{}' has empty remote URL", server.name));
            }
        }

        // Validate local server
        if is_local {
            if server.packages.len() != 1 {
                return Err(eyre::eyre!(
                    "Server '{}' must have exactly one package configuration, found {}",
                    server.name,
                    server.packages.len()
                ));
            }
            let package = &server.packages[0];

            // Validate registry type
            if package.registry_type != "npm" && package.registry_type != "pypi" && package.registry_type != "oci" {
                return Err(eyre::eyre!(
                    "Server '{}' has invalid registry type '{}', must be 'npm', 'pypi', or 'oci'",
                    server.name,
                    package.registry_type
                ));
            }

            // Validate identifier
            if package.identifier.is_empty() {
                return Err(eyre::eyre!("Server '{}' has empty package identifier", server.name));
            }

            // Validate transport
            if package.transport.transport_type != "stdio" {
                return Err(eyre::eyre!(
                    "Server '{}' has invalid transport type '{}', must be 'stdio'",
                    server.name,
                    package.transport.transport_type
                ));
            }

            // Validate arguments
            for arg in &package.runtime_arguments {
                if arg.arg_type != "positional" {
                    return Err(eyre::eyre!(
                        "Server '{}' has invalid runtime argument type '{}', must be 'positional'",
                        server.name,
                        arg.arg_type
                    ));
                }
            }
            for arg in &package.package_arguments {
                if arg.arg_type != "positional" {
                    return Err(eyre::eyre!(
                        "Server '{}' has invalid package argument type '{}', must be 'positional'",
                        server.name,
                        arg.arg_type
                    ));
                }
            }
        }

        Ok(())
    }

    /// Get a server by name
    pub fn get_server(&self, name: &str) -> Option<&McpServerDefinition> {
        self.servers
            .iter()
            .find(|entry| entry.server.name == name)
            .map(|entry| &entry.server)
    }
}

/// Client for fetching MCP server definitions from a registry
pub struct McpRegistryClient {
    http_client: reqwest::Client,
}

impl McpRegistryClient {
    /// Create a new registry client
    pub fn new() -> Self {
        Self {
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()
                .unwrap_or_default(),
        }
    }

    /// Fetch server definitions from the registry URL
    pub async fn fetch_registry(&self, registry_url: &str) -> Result<McpRegistryResponse> {
        tracing::debug!("Fetching MCP registry from: {}", registry_url);

        let response = self.http_client.get(registry_url).send().await?;

        if !response.status().is_success() {
            return Err(eyre::eyre!("Failed to fetch MCP registry: HTTP {}", response.status()));
        }

        let json_text = response
            .text()
            .await
            .map_err(|e| eyre::eyre!("Failed to read response body: {}", e))?;

        let registry: McpRegistryResponse =
            serde_json::from_str(&json_text).map_err(|e| eyre::eyre!("Failed to parse registry JSON: {}", e))?;

        // Validate the registry structure
        registry
            .validate()
            .map_err(|e| eyre::eyre!("Registry validation failed: {}", e))?;

        tracing::debug!("Fetched and validated {} servers from registry", registry.servers.len());

        Ok(registry)
    }

    /// Fetch and cache registry data with TTL
    pub async fn fetch_with_cache(
        &self,
        registry_url: &str,
        cache: &mut Option<CachedRegistry>,
        ttl_hours: i64,
    ) -> Result<McpRegistryResponse> {
        // Check if cache is valid (not stale and same URL)
        if let Some(cached) = cache {
            if !cached.should_refresh(registry_url, ttl_hours) {
                tracing::debug!("Using cached registry data");
                return Ok(cached.data.clone());
            } else if cached.is_url_changed(registry_url) {
                tracing::debug!(
                    "Registry URL changed from '{}' to '{}', invalidating cache",
                    cached.source_url,
                    registry_url
                );
            } else {
                tracing::debug!("Cache is stale, refreshing");
            }
        }

        // Fetch fresh data
        let data = self.fetch_registry(registry_url).await?;

        // Update cache with source URL
        *cache = Some(CachedRegistry {
            data: data.clone(),
            fetched_at: time::OffsetDateTime::now_utc(),
            source_url: registry_url.to_string(),
        });

        Ok(data)
    }
}

impl Default for McpRegistryClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Cached registry data with timestamp
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedRegistry {
    pub data: McpRegistryResponse,
    pub fetched_at: time::OffsetDateTime,
    pub source_url: String,
}

impl CachedRegistry {
    /// Check if cache is stale (older than TTL)
    pub fn is_stale(&self, ttl_hours: i64) -> bool {
        let now = time::OffsetDateTime::now_utc();
        let elapsed = now - self.fetched_at;
        elapsed.whole_hours() >= ttl_hours
    }

    /// Check if the cache is for a different URL
    pub fn is_url_changed(&self, current_url: &str) -> bool {
        self.source_url != current_url
    }

    /// Check if cache should be refreshed (either stale or URL changed)
    pub fn should_refresh(&self, current_url: &str, ttl_hours: i64) -> bool {
        self.is_url_changed(current_url) || self.is_stale(ttl_hours)
    }
}

/// Result of processing MCP servers
pub struct ProcessServersResult {
    pub servers: std::collections::HashMap<String, crate::cli::chat::tools::custom_tool::CustomToolConfig>,
    pub ignored_servers: Vec<String>,
}

/// Process MCP servers based on registry mode
pub fn process_mcp_servers(
    agent_servers: &std::collections::HashMap<String, crate::cli::chat::tools::custom_tool::CustomToolConfig>,
    registry: Option<&McpRegistryResponse>,
) -> Result<ProcessServersResult> {
    let mut processed_servers = std::collections::HashMap::new();
    let mut ignored_servers = Vec::new();

    if let Some(registry_data) = registry {
        // Registry mode: process servers that are either explicitly registry type OR exist in registry
        for (server_name, agent_config) in agent_servers {
            // Check if server exists in registry
            let registry_server = registry_data.get_server(server_name);

            // Determine if this server should be processed in registry mode
            let should_process = agent_config.is_registry_type() || registry_server.is_some();

            if !should_process {
                tracing::debug!("Registry mode: ignoring server '{}' (not in registry)", server_name);
                ignored_servers.push(server_name.clone());
                continue;
            }

            // Get registry server definition
            let registry_server = match registry_server {
                Some(server) => server,
                None => {
                    // This shouldn't happen due to the check above, but handle it gracefully
                    tracing::warn!("Registry server '{}' not found in registry, ignoring", server_name);
                    ignored_servers.push(server_name.clone());
                    continue;
                },
            };

            // Convert registry definition to CustomToolConfig, preserving user overrides
            let config = convert_registry_to_config(registry_server, agent_config)?;
            processed_servers.insert(server_name.clone(), config);

            if agent_config.is_from_legacy_mcp_json {
                tracing::debug!("Loaded legacy registry server: {}", server_name);
            } else {
                tracing::debug!("Loaded registry server: {}", server_name);
            }
        }
    } else {
        // Non-registry mode: only process stdio/http servers
        for (server_name, agent_config) in agent_servers {
            if agent_config.is_registry_type() {
                tracing::debug!("Non-registry mode: ignoring registry-type server '{}'", server_name);
                ignored_servers.push(server_name.clone());
                continue;
            }

            processed_servers.insert(server_name.clone(), agent_config.clone());
            tracing::debug!("Loaded user-defined server: {}", server_name);
        }
    }

    Ok(ProcessServersResult {
        servers: processed_servers,
        ignored_servers,
    })
}

/// Convert registry server definition to CustomToolConfig
/// Uses registry definition as the base, with valid agent overrides per spec.
///
/// Valid override fields for registry entries:
/// - type: Must be "registry" (required)
/// - timeout: Optional timeout for MCP requests
/// - headers: Optional HTTP headers (for remote servers only)
/// - env: Optional environment variables (for local servers only)
pub fn convert_registry_to_config(
    registry_server: &McpServerDefinition,
    agent_config: &crate::cli::chat::tools::custom_tool::CustomToolConfig,
) -> Result<crate::cli::chat::tools::custom_tool::CustomToolConfig> {
    use crate::cli::chat::tools::custom_tool::CustomToolConfig;

    let mut config = CustomToolConfig {
        transport_type: None, // Will be inferred
        url: String::new(),
        headers: std::collections::HashMap::new(),
        oauth_scopes: crate::cli::chat::tools::custom_tool::get_default_scopes(),
        oauth: None,
        command: String::new(),
        args: Vec::new(),
        env: None,
        timeout: agent_config.timeout,
        disabled: false,
        disabled_tools: Vec::new(),
        is_from_legacy_mcp_json: false,
    };

    // Check if it's a remote or local server
    let is_remote = !registry_server.remotes.is_empty();
    let is_local = !registry_server.packages.is_empty();

    if is_remote {
        // Remote server: use registry URL and headers
        let remote = &registry_server.remotes[0];
        config.url = remote.url.clone();

        // Start with registry headers
        for header in &remote.headers {
            config.headers.insert(header.name.clone(), header.value.clone());
        }

        // Override with agent headers (agent wins)
        for (key, value) in &agent_config.headers {
            config.headers.insert(key.clone(), value.clone());
        }
    } else if is_local {
        // Local server: build command from registry package definition
        let package = &registry_server.packages[0];

        // Build command and args based on registry type
        match package.registry_type.as_str() {
            "npm" => {
                // NPM: npx -y <runtimeArguments> <identifier>@<version> <packageArguments>
                config.command = "npx".to_string();
                config.args.push("-y".to_string());

                // Add runtime arguments
                config
                    .args
                    .extend(package.runtime_arguments.iter().map(|arg| arg.value.clone()));

                // Add package identifier with version
                config
                    .args
                    .push(format!("{}@{}", package.identifier, registry_server.version));

                // Add package arguments
                config
                    .args
                    .extend(package.package_arguments.iter().map(|arg| arg.value.clone()));

                let mut env_map = std::collections::HashMap::new();
                if let Some(ref registry_url) = package.registry_base_url {
                    env_map.insert("NPM_CONFIG_REGISTRY".to_string(), registry_url.clone());
                }

                for env_var in &package.environment_variables {
                    env_map.insert(env_var.name.clone(), env_var.value.clone());
                }

                // Override with agent environment variables (agent wins)
                if let Some(ref agent_env) = agent_config.env {
                    for (key, value) in agent_env {
                        env_map.insert(key.clone(), value.clone());
                    }
                }

                if !env_map.is_empty() {
                    config.env = Some(env_map);
                }
            },
            "pypi" => {
                // PyPI: uvx --default-index=<registryBaseUrl> <runtimeArguments> <identifier>@<version>
                // <packageArguments>
                config.command = "uvx".to_string();

                // Add --default-index if registryBaseUrl is specified
                if let Some(ref registry_url) = package.registry_base_url {
                    config.args.push(format!("--default-index={registry_url}"));
                }

                // Add runtime arguments
                config
                    .args
                    .extend(package.runtime_arguments.iter().map(|arg| arg.value.clone()));

                // Add package identifier with version
                config
                    .args
                    .push(format!("{}@{}", package.identifier, registry_server.version));

                // Add package arguments
                config
                    .args
                    .extend(package.package_arguments.iter().map(|arg| arg.value.clone()));

                let mut env_map = std::collections::HashMap::new();
                for env_var in &package.environment_variables {
                    env_map.insert(env_var.name.clone(), env_var.value.clone());
                }

                // Override with agent environment variables (agent wins)
                if let Some(ref agent_env) = agent_config.env {
                    for (key, value) in agent_env {
                        env_map.insert(key.clone(), value.clone());
                    }
                }

                if !env_map.is_empty() {
                    config.env = Some(env_map);
                }
            },
            "oci" => {
                // OCI: docker run <runtimeArguments> <env-flags> <registryBaseUrl>/<identifier>:<version>
                // <packageArguments>
                config.command = "docker".to_string();
                config.args.push("run".to_string());

                // Add runtime arguments first
                config
                    .args
                    .extend(package.runtime_arguments.iter().map(|arg| arg.value.clone()));

                // Collect all environment variables
                let mut all_env = std::collections::HashMap::new();

                // Add registry env vars first
                for env_var in &package.environment_variables {
                    if !env_var.name.trim().is_empty() && !env_var.value.trim().is_empty() {
                        all_env.insert(env_var.name.clone(), env_var.value.clone());
                    }
                }

                // Override with agent environment variables (agent wins)
                if let Some(ref agent_env) = agent_config.env {
                    for (key, value) in agent_env {
                        if !key.trim().is_empty() && !value.trim().is_empty() {
                            all_env.insert(key.clone(), value.clone());
                        }
                    }
                }

                // Add all environment variables as -e flags before the image reference
                for (key, value) in &all_env {
                    config.args.push("-e".to_string());
                    config.args.push(format!("{key}={value}"));
                }

                // Add image reference: <registryBaseUrl>/<identifier>:<version>
                let image_ref = if let Some(ref registry_url) = package.registry_base_url {
                    // Check if identifier already contains a tag
                    if package.identifier.contains(':') {
                        format!("{}/{}", registry_url, package.identifier)
                    } else {
                        format!("{}/{}:{}", registry_url, package.identifier, registry_server.version)
                    }
                } else {
                    // Check if identifier already contains a tag
                    if package.identifier.contains(':') {
                        package.identifier.clone()
                    } else {
                        format!("{}:{}", package.identifier, registry_server.version)
                    }
                };
                config.args.push(image_ref);

                // Add package arguments
                config
                    .args
                    .extend(package.package_arguments.iter().map(|arg| arg.value.clone()));

                // Don't set config.env for OCI since we're using -e flags instead
            },
            _ => {
                return Err(eyre::eyre!(
                    "Unknown registry type '{}' for server '{}'",
                    package.registry_type,
                    registry_server.name
                ));
            },
        }
    }

    Ok(config)
}

/// Apply registry filtering to an agent's MCP servers
/// This validates registry servers exist but keeps original minimal configuration
pub fn apply_registry_filtering_to_agent(
    agent: &mut crate::cli::agent::Agent,
    registry: &McpRegistryResponse,
) -> Result<()> {
    {
        let registry_data = registry;
        let original_servers: Vec<&str> = agent.mcp_servers.mcp_servers.keys().map(|s| s.as_str()).collect();
        tracing::debug!(
            "Before registry filtering - agent '{}' has {} servers: {:?}",
            agent.name,
            original_servers.len(),
            original_servers
        );

        match process_mcp_servers(&agent.mcp_servers.mcp_servers, Some(registry_data)) {
            Ok(result) => {
                // Don't overwrite agent config - just validate and filter tools
                let valid_server_names: std::collections::HashSet<&str> =
                    result.servers.keys().map(|s| s.as_str()).collect();

                // Remove invalid registry servers from agent config (keep original minimal configs for valid ones)
                let mut servers_to_remove = Vec::new();
                for (server_name, config) in &agent.mcp_servers.mcp_servers {
                    if config.is_registry_type() && !valid_server_names.contains(server_name.as_str()) {
                        servers_to_remove.push(server_name.clone());
                    }
                }

                for server_name in servers_to_remove {
                    agent.mcp_servers.mcp_servers.remove(&server_name);
                }

                // Filter tools to only include valid servers
                agent.tools = filter_tools_by_registry(&agent.tools, &valid_server_names);

                tracing::debug!(
                    "Applied registry filtering to agent '{}': {} servers validated, {} ignored. Agent config preserved with minimal registry entries.",
                    agent.name,
                    valid_server_names.len(),
                    result.ignored_servers.len()
                );
            },
            Err(e) => {
                tracing::error!("Failed to apply registry filtering to agent '{}': {}", agent.name, e);
                // On error, clear MCP servers to avoid using invalid configs
                agent.mcp_servers.mcp_servers.clear();
                return Err(e);
            },
        }
    }
    Ok(())
}

/// Filter tools to only include those from valid registry servers
pub fn filter_tools_by_registry(tools: &[String], valid_server_names: &std::collections::HashSet<&str>) -> Vec<String> {
    tools
        .iter()
        .filter(|tool| {
            // Allow wildcard
            if *tool == "*" {
                return true;
            }

            // Check if tool is prefixed with a valid server name
            if let Some(stripped) = tool.strip_prefix('@')
                && let Some(slash_pos) = stripped.find('/')
            {
                let server_name = &stripped[..slash_pos];
                if valid_server_names.contains(server_name) {
                    return true;
                } else {
                    tracing::debug!(
                        "Filtering out tool '{}' - server '{}' not in registry",
                        tool,
                        server_name
                    );
                    return false;
                }
            }

            // Non-prefixed tools are allowed (native tools)
            true
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::chat::tools::custom_tool::CustomToolConfig;

    #[test]
    fn test_process_mcp_servers_registry_mode() {
        let registry_json = r#"{
            "servers": [{
                "server": {
                    "name": "test-server",
                    "description": "Test",
                    "version": "1.0.0",
                    "remotes": [{"type": "sse", "url": "https://example.com"}]
                }
            }]
        }"#;
        let registry: McpRegistryResponse = serde_json::from_str(registry_json).unwrap();

        let mut agent_servers = std::collections::HashMap::new();
        agent_servers.insert("test-server".to_string(), CustomToolConfig {
            transport_type: Some("registry".to_string()),
            url: String::new(),
            headers: std::collections::HashMap::new(),
            oauth_scopes: vec![],
            oauth: None,
            command: String::new(),
            args: vec![],
            env: None,
            timeout: 120000,
            disabled: false,
            disabled_tools: vec![],
            is_from_legacy_mcp_json: false,
        });

        let result = process_mcp_servers(&agent_servers, Some(&registry)).unwrap();
        assert_eq!(result.servers.len(), 1);
        assert!(result.servers.contains_key("test-server"));
    }

    #[test]
    fn test_process_mcp_servers_non_registry_mode() {
        let mut agent_servers = std::collections::HashMap::new();
        agent_servers.insert("stdio-server".to_string(), CustomToolConfig {
            transport_type: None,
            url: String::new(),
            headers: std::collections::HashMap::new(),
            oauth_scopes: vec![],
            oauth: None,
            command: "node".to_string(),
            args: vec!["server.js".to_string()],
            env: None,
            timeout: 120000,
            disabled: false,
            disabled_tools: vec![],
            is_from_legacy_mcp_json: false,
        });

        let result = process_mcp_servers(&agent_servers, None).unwrap();
        assert_eq!(result.servers.len(), 1);
        assert!(result.servers.contains_key("stdio-server"));
    }

    #[test]
    fn test_npm_registry_conversion() {
        let registry_json = r#"{
            "servers": [{
                "server": {
                    "name": "npm-server",
                    "description": "NPM test",
                    "version": "1.0.2",
                    "packages": [{
                        "registryType": "npm",
                        "registryBaseUrl": "https://npm.acme.com",
                        "identifier": "@acme/server",
                        "transport": {"type": "stdio"},
                        "runtimeArguments": [{"type": "positional", "value": "--quiet"}],
                        "packageArguments": [{"type": "positional", "value": "--readonly"}],
                        "environmentVariables": [{"name": "DEBUG", "value": "true"}]
                    }]
                }
            }]
        }"#;
        let registry: McpRegistryResponse = serde_json::from_str(registry_json).unwrap();

        let mut agent_servers = std::collections::HashMap::new();
        agent_servers.insert("npm-server".to_string(), CustomToolConfig {
            transport_type: Some("registry".to_string()),
            url: String::new(),
            headers: std::collections::HashMap::new(),
            oauth_scopes: vec![],
            oauth: None,
            command: String::new(),
            args: vec![],
            env: None,
            timeout: 120000,
            disabled: false,
            disabled_tools: vec![],
            is_from_legacy_mcp_json: false,
        });

        let result = process_mcp_servers(&agent_servers, Some(&registry)).unwrap();
        let config = result.servers.get("npm-server").unwrap();

        assert_eq!(config.command, "npx");
        assert_eq!(config.args, vec!["-y", "--quiet", "@acme/server@1.0.2", "--readonly"]);
        assert_eq!(
            config.env.as_ref().unwrap().get("NPM_CONFIG_REGISTRY").unwrap(),
            "https://npm.acme.com"
        );
    }

    #[test]
    fn test_filter_tools_by_registry() {
        let mut valid_servers = std::collections::HashSet::new();
        valid_servers.insert("server1");

        let tools = vec![
            "*".to_string(),
            "@server1/tool".to_string(),
            "@invalid/tool".to_string(),
            "native_tool".to_string(),
        ];

        let filtered = filter_tools_by_registry(&tools, &valid_servers);

        assert_eq!(filtered.len(), 3);
        assert!(filtered.contains(&"*".to_string()));
        assert!(filtered.contains(&"@server1/tool".to_string()));
        assert!(filtered.contains(&"native_tool".to_string()));
        assert!(!filtered.contains(&"@invalid/tool".to_string()));
    }

    #[test]
    fn test_registry_server_not_found() {
        // When a registry-type server is not found in the registry, it should be skipped
        let registry_json = r#"{
            "servers": [{
                "server": {
                    "name": "existing-server",
                    "description": "Test",
                    "version": "1.0.0",
                    "remotes": [{"type": "sse", "url": "https://example.com"}]
                }
            }]
        }"#;
        let registry: McpRegistryResponse = serde_json::from_str(registry_json).unwrap();

        let mut agent_servers = std::collections::HashMap::new();
        agent_servers.insert("missing-server".to_string(), CustomToolConfig {
            transport_type: Some("registry".to_string()),
            url: String::new(),
            headers: std::collections::HashMap::new(),
            oauth_scopes: vec![],
            oauth: None,
            command: String::new(),
            args: vec![],
            env: None,
            timeout: 120000,
            disabled: false,
            disabled_tools: vec![],
            is_from_legacy_mcp_json: false,
        });

        let result = process_mcp_servers(&agent_servers, Some(&registry)).unwrap();
        // Server not found should result in empty map (server skipped)
        assert_eq!(result.servers.len(), 0);
    }

    #[test]
    fn test_invalid_registry_json() {
        // Invalid JSON should fail to parse
        let invalid_json = r#"{ "not": "valid" }"#;
        let result: Result<McpRegistryResponse, _> = serde_json::from_str(invalid_json);
        assert!(result.is_err());
    }

    #[test]
    fn test_registry_validation_empty_servers() {
        // Registry with no servers should fail validation
        let response = McpRegistryResponse { servers: vec![] };
        assert!(response.validate().is_err());
    }

    #[test]
    fn test_registry_validation_duplicate_names() {
        // Registry with duplicate server names should fail validation
        let json = r#"{
            "servers": [
                {
                    "server": {
                        "name": "duplicate",
                        "description": "First",
                        "version": "1.0.0",
                        "remotes": [{"type": "sse", "url": "https://example.com"}]
                    }
                },
                {
                    "server": {
                        "name": "duplicate",
                        "description": "Second",
                        "version": "1.0.0",
                        "remotes": [{"type": "sse", "url": "https://example2.com"}]
                    }
                }
            ]
        }"#;

        let response: McpRegistryResponse = serde_json::from_str(json).unwrap();
        assert!(response.validate().is_err());
    }

    #[test]
    fn test_cache_staleness() {
        // Test that cache correctly identifies stale data
        let cache = CachedRegistry {
            data: McpRegistryResponse { servers: vec![] },
            fetched_at: time::OffsetDateTime::now_utc() - time::Duration::hours(25),
            source_url: "test".to_string(),
        };

        assert!(cache.is_stale(MCP_CACHE_TTL_HOURS)); // 25 hours old, TTL is 24 hours
        assert!(!cache.is_stale(48)); // 25 hours old, but TTL is 48 hours
    }

    #[test]
    fn test_unknown_registry_type() {
        // Unknown registry type should return an error
        let registry_json = r#"{
            "servers": [{
                "server": {
                    "name": "unknown-type",
                    "description": "Test",
                    "version": "1.0.0",
                    "packages": [{
                        "registryType": "unknown",
                        "identifier": "test",
                        "transport": {"type": "stdio"}
                    }]
                }
            }]
        }"#;
        let registry: McpRegistryResponse = serde_json::from_str(registry_json).unwrap();

        let mut agent_servers = std::collections::HashMap::new();
        agent_servers.insert("unknown-type".to_string(), CustomToolConfig {
            transport_type: Some("registry".to_string()),
            url: String::new(),
            headers: std::collections::HashMap::new(),
            oauth_scopes: vec![],
            oauth: None,
            command: String::new(),
            args: vec![],
            env: None,
            timeout: 120000,
            disabled: false,
            disabled_tools: vec![],
            is_from_legacy_mcp_json: false,
        });

        let result = process_mcp_servers(&agent_servers, Some(&registry));
        // Should return error for unknown registry type
        assert!(result.is_err());
    }

    #[test]
    fn test_mutually_exclusive_modes() {
        // Test that registry and non-registry servers are mutually exclusive
        let registry_json = r#"{
            "servers": [{
                "server": {
                    "name": "registry-server",
                    "description": "Test",
                    "version": "1.0.0",
                    "remotes": [{"type": "sse", "url": "https://example.com"}]
                }
            }]
        }"#;
        let registry: McpRegistryResponse = serde_json::from_str(registry_json).unwrap();

        let mut agent_servers = std::collections::HashMap::new();

        // Add both registry and stdio servers
        agent_servers.insert("registry-server".to_string(), CustomToolConfig {
            transport_type: Some("registry".to_string()),
            url: String::new(),
            headers: std::collections::HashMap::new(),
            oauth_scopes: vec![],
            oauth: None,
            command: String::new(),
            args: vec![],
            env: None,
            timeout: 120000,
            disabled: false,
            disabled_tools: vec![],
            is_from_legacy_mcp_json: false,
        });

        agent_servers.insert("stdio-server".to_string(), CustomToolConfig {
            transport_type: None,
            url: String::new(),
            headers: std::collections::HashMap::new(),
            oauth_scopes: vec![],
            oauth: None,
            command: "node".to_string(),
            args: vec!["server.js".to_string()],
            env: None,
            timeout: 120000,
            disabled: false,
            disabled_tools: vec![],
            is_from_legacy_mcp_json: false,
        });

        // In registry mode, only registry server should be loaded
        let result_registry = process_mcp_servers(&agent_servers, Some(&registry)).unwrap();
        assert_eq!(result_registry.servers.len(), 1);
        assert!(result_registry.servers.contains_key("registry-server"));
        assert!(!result_registry.servers.contains_key("stdio-server"));

        // In non-registry mode, only stdio server should be loaded
        let result_non_registry = process_mcp_servers(&agent_servers, None).unwrap();
        assert_eq!(result_non_registry.servers.len(), 1);
        assert!(!result_non_registry.servers.contains_key("registry-server"));
        assert!(result_non_registry.servers.contains_key("stdio-server"));
    }

    #[test]
    fn test_config_override_remote_server() {
        // Test that agent config overrides registry config for remote servers
        let registry_json = r#"{
            "version": "1.0.0",
            "servers": [
                {
                    "server": {
                        "name": "remote-server",
                        "version": "1.0.0",
                        "description": "Test remote server",
                        "remotes": [
                            {
                                "type": "http",
                                "url": "https://api.example.com/mcp",
                                "headers": [
                                    {"name": "Content-Type", "value": "application/json"},
                                    {"name": "User-Agent", "value": "MCP-Client/1.0"}
                                ]
                            }
                        ]
                    }
                }
            ]
        }"#;

        let registry: McpRegistryResponse = serde_json::from_str(registry_json).unwrap();

        // Agent config with overrides
        let mut agent_headers = std::collections::HashMap::new();
        agent_headers.insert("Authorization".to_string(), "Bearer secret-token".to_string());
        agent_headers.insert("User-Agent".to_string(), "MyApp/2.0".to_string()); // Override registry value

        let agent_config = CustomToolConfig {
            transport_type: Some("registry".to_string()), // Must be registry type
            url: String::new(),                           // Will be overridden by registry
            headers: agent_headers,
            oauth_scopes: vec![],
            oauth: None,
            command: String::new(),
            args: vec![],
            env: None,
            timeout: 30000, // Agent timeout
            disabled: false,
            disabled_tools: vec!["tool1".to_string()], // Agent disabled tools
            is_from_legacy_mcp_json: true,             // Agent setting
        };

        let mut agent_servers = std::collections::HashMap::new();
        agent_servers.insert("remote-server".to_string(), agent_config);

        let result = process_mcp_servers(&agent_servers, Some(&registry)).unwrap();
        let config = result.servers.get("remote-server").unwrap();

        // Registry should control URL
        assert_eq!(config.url, "https://api.example.com/mcp");

        // Headers should be merged with agent winning conflicts
        assert_eq!(config.headers.get("Content-Type").unwrap(), "application/json"); // From registry
        assert_eq!(config.headers.get("User-Agent").unwrap(), "MyApp/2.0"); // Agent override
        assert_eq!(config.headers.get("Authorization").unwrap(), "Bearer secret-token"); // Agent addition

        // Only timeout and headers can be overridden for remote servers
        assert_eq!(config.timeout, 30000);
        assert!(config.disabled_tools.is_empty()); // Registry controls disabled_tools
        assert!(!config.is_from_legacy_mcp_json); // Registry controls this flag
    }

    #[test]
    fn test_config_override_local_server() {
        // Test that agent config overrides registry config for local servers
        let registry_json = r#"{
            "version": "1.0.0",
            "servers": [
                {
                    "server": {
                        "name": "npm-server",
                        "version": "1.2.3",
                        "description": "Test NPM server",
                        "packages": [
                            {
                                "registryType": "npm",
                                "identifier": "@test/mcp-server",
                                "registryBaseUrl": "https://registry.npmjs.org",
                                "transport": {
                                    "type": "stdio"
                                },
                                "runtimeArguments": [
                                    {"type": "positional", "value": "--verbose"}
                                ],
                                "packageArguments": [
                                    {"type": "positional", "value": "--port=3000"}
                                ],
                                "environmentVariables": [
                                    {"name": "NODE_ENV", "value": "production"},
                                    {"name": "LOG_LEVEL", "value": "info"}
                                ]
                            }
                        ]
                    }
                }
            ]
        }"#;

        let registry: McpRegistryResponse = serde_json::from_str(registry_json).unwrap();

        // Agent config with environment overrides
        let mut agent_env = std::collections::HashMap::new();
        agent_env.insert("API_KEY".to_string(), "secret-key".to_string()); // Agent addition
        agent_env.insert("LOG_LEVEL".to_string(), "debug".to_string()); // Override registry value

        let agent_config = CustomToolConfig {
            transport_type: Some("registry".to_string()), // Must be registry type
            url: String::new(),
            headers: std::collections::HashMap::new(),
            oauth_scopes: vec![],
            oauth: None,
            command: String::new(), // Will be overridden by registry
            args: vec![],           // Will be overridden by registry
            env: Some(agent_env),
            timeout: 45000, // Agent timeout
            disabled: true, // Agent disabled
            disabled_tools: vec!["dangerous-tool".to_string()],
            is_from_legacy_mcp_json: false,
        };

        let mut agent_servers = std::collections::HashMap::new();
        agent_servers.insert("npm-server".to_string(), agent_config);

        let result = process_mcp_servers(&agent_servers, Some(&registry)).unwrap();
        let config = result.servers.get("npm-server").unwrap();

        // Registry should control command and args
        assert_eq!(config.command, "npx");
        assert!(config.args.contains(&"-y".to_string()));
        assert!(config.args.contains(&"@test/mcp-server@1.2.3".to_string()));
        assert!(config.args.contains(&"--verbose".to_string()));
        assert!(config.args.contains(&"--port=3000".to_string()));

        // Environment should be merged with agent winning conflicts
        let env = config.env.as_ref().unwrap();
        assert_eq!(env.get("NODE_ENV").unwrap(), "production"); // From registry
        assert_eq!(env.get("LOG_LEVEL").unwrap(), "debug"); // Agent override
        assert_eq!(env.get("API_KEY").unwrap(), "secret-key"); // Agent addition
        assert_eq!(env.get("NPM_CONFIG_REGISTRY").unwrap(), "https://registry.npmjs.org"); // From registry

        // Agent timeout should be preserved
        assert_eq!(config.timeout, 45000);
    }
}
/// Display registry error message to any writer that implements Write
pub fn display_registry_error_to_writer<W: std::io::Write>(
    writer: &mut W,
    url: &str,
    error_type: &RegistryErrorType,
    warning_prefix: &str,
    retry_message: &str,
) -> std::io::Result<()> {
    use crossterm::{
        queue,
        style,
    };

    use crate::theme::StyledText;

    queue!(
        writer,
        StyledText::error_fg(),
        style::Print(warning_prefix),
        StyledText::reset(),
        style::Print("MCP is disabled because the configured registry at "),
        StyledText::brand_fg(),
        style::Print(url),
        StyledText::reset(),
    )?;

    match error_type {
        RegistryErrorType::NetworkConnectivity => {
            queue!(writer, style::Print(" is not reachable. Check your network connection"),)?;
        },
        RegistryErrorType::RegistryData => {
            queue!(
                writer,
                style::Print(" contains invalid data. Contact your administrator"),
            )?;
        },
    }

    queue!(
        writer,
        style::Print(".\n"),
        style::Print(retry_message),
        style::Print("\n\n"),
    )?;

    writer.flush()?;
    Ok(())
}
#[tokio::test]
async fn test_registry_sync_behavior() {
    use std::collections::HashMap;

    use crate::cli::chat::tools::custom_tool::CustomToolConfig;

    // Test registry syncing behavior including cache refresh and server updates

    // Initial registry with one server
    let initial_registry_json = r#"{
            "servers": [{
                "server": {
                    "name": "test-server",
                    "description": "Initial test server",
                    "version": "1.0.0",
                    "packages": [{
                        "registryType": "npm",
                        "identifier": "test-package",
                        "transport": {"type": "stdio"}
                    }]
                }
            }]
        }"#;

    // Updated registry with version change and new server
    let updated_registry_json = r#"{
            "servers": [
                {
                    "server": {
                        "name": "test-server",
                        "description": "Updated test server",
                        "version": "2.0.0",
                        "packages": [{
                            "registryType": "npm",
                            "identifier": "test-package",
                            "transport": {"type": "stdio"}
                        }]
                    }
                },
                {
                    "server": {
                        "name": "new-server",
                        "description": "Newly added server",
                        "version": "1.0.0",
                        "packages": [{
                            "registryType": "npm",
                            "identifier": "new-package",
                            "transport": {"type": "stdio"}
                        }]
                    }
                }
            ]
        }"#;

    // Parse registries
    let initial_registry: McpRegistryResponse = serde_json::from_str(initial_registry_json).unwrap();
    let updated_registry: McpRegistryResponse = serde_json::from_str(updated_registry_json).unwrap();

    // Test 1: Initial cache creation
    let now = time::OffsetDateTime::now_utc();

    // Simulate initial cache population
    let cache = Some(CachedRegistry {
        data: initial_registry.clone(),
        fetched_at: now,
        source_url: "test".to_string(),
    });

    // Verify cache is fresh
    assert!(!cache.as_ref().unwrap().is_stale(MCP_CACHE_TTL_HOURS));

    // Test 2: Cache staleness detection
    // Simulate cache becoming stale (6 minutes old)
    let stale_cache = CachedRegistry {
        data: initial_registry.clone(),
        fetched_at: now - time::Duration::hours(25),
        source_url: "test".to_string(),
    };
    assert!(stale_cache.is_stale(MCP_CACHE_TTL_HOURS));

    // Test 3: Server version change detection
    let mut cached_versions = HashMap::new();
    cached_versions.insert("test-server".to_string(), "1.0.0".to_string());

    // Check for version changes between registries
    let initial_server = initial_registry.get_server("test-server").unwrap();
    let updated_server = updated_registry.get_server("test-server").unwrap();

    assert_eq!(initial_server.version, "1.0.0");
    assert_eq!(updated_server.version, "2.0.0");

    // Simulate version change detection
    let has_version_change = cached_versions
        .get("test-server")
        .map(|cached_version| cached_version != &updated_server.version)
        .unwrap_or(false);
    assert!(has_version_change);

    // Test 4: New server detection
    assert!(initial_registry.get_server("new-server").is_none());
    assert!(updated_registry.get_server("new-server").is_some());

    // Test 5: Server processing with registry updates
    let mut agent_servers = HashMap::new();

    // Add initial server to agent (configured as registry type)
    let initial_config = CustomToolConfig {
        transport_type: Some("registry".to_string()),
        url: String::new(),
        headers: HashMap::new(),
        oauth_scopes: vec![],
        oauth: None,
        command: String::new(), // Will be set by registry conversion
        args: vec![],
        env: None,
        timeout: 30000,
        disabled: false,
        disabled_tools: vec![],
        is_from_legacy_mcp_json: false,
    };
    agent_servers.insert("test-server".to_string(), initial_config);

    // Process with initial registry
    let initial_result = process_mcp_servers(&agent_servers, Some(&initial_registry)).unwrap();
    assert_eq!(initial_result.servers.len(), 1);
    assert!(initial_result.ignored_servers.is_empty());

    // Process with updated registry (should update the server config)
    let updated_result = process_mcp_servers(&agent_servers, Some(&updated_registry)).unwrap();
    assert_eq!(updated_result.servers.len(), 1);
    assert!(updated_result.ignored_servers.is_empty());

    // Verify the server config was updated to new version
    let updated_config = updated_result.servers.get("test-server").unwrap();
    // For NPM packages, the command is "npx" and the package is in args
    assert_eq!(updated_config.command, "npx");

    // The args should contain the package with updated version
    let expected_package = "test-package@2.0.0";
    assert!(
        updated_config.args.iter().any(|arg| arg == &expected_package),
        "Expected args to contain '{}', but got: {:?}",
        expected_package,
        updated_config.args
    );

    // Test 6: Registry validation during sync
    // Test with invalid registry (should fail validation)
    let invalid_registry = McpRegistryResponse { servers: vec![] };
    assert!(invalid_registry.validate().is_err());

    // Test with valid registry (should pass validation)
    assert!(updated_registry.validate().is_ok());

    // Test 7: Tool filtering with registry updates
    let initial_tools = vec![
        "@test-server/tool1".to_string(),
        "@invalid-server/tool2".to_string(),
        "native-tool".to_string(),
    ];

    // Use actual server names from processing results
    let initial_valid_servers: std::collections::HashSet<&str> =
        initial_result.servers.keys().map(|s| s.as_str()).collect();
    let updated_valid_servers: std::collections::HashSet<&str> =
        updated_result.servers.keys().map(|s| s.as_str()).collect();

    let initial_filtered = filter_tools_by_registry(&initial_tools, &initial_valid_servers);
    // Should keep @test-server/tool1 (valid) and native-tool (non-prefixed), filter out
    // @invalid-server/tool2
    assert_eq!(initial_filtered.len(), 2);
    assert!(initial_filtered.contains(&"@test-server/tool1".to_string()));
    assert!(initial_filtered.contains(&"native-tool".to_string()));
    assert!(!initial_filtered.contains(&"@invalid-server/tool2".to_string()));

    let updated_filtered = filter_tools_by_registry(&initial_tools, &updated_valid_servers);
    // Same result since updated_valid_servers still only contains "test-server"
    assert_eq!(updated_filtered.len(), 2);
    assert!(updated_filtered.contains(&"@test-server/tool1".to_string()));
    assert!(updated_filtered.contains(&"native-tool".to_string()));

    // Simulate cache refresh (make it 6 minutes old to trigger refresh)
    let mut test_cache = Some(CachedRegistry {
        data: initial_registry,
        fetched_at: now - time::Duration::hours(25), // Stale
        source_url: "test".to_string(),
    });

    if test_cache.as_ref().unwrap().is_stale(MCP_CACHE_TTL_HOURS) {
        test_cache = Some(CachedRegistry {
            data: updated_registry.clone(),
            fetched_at: time::OffsetDateTime::now_utc(),
            source_url: "test".to_string(),
        });
    }

    // Verify cache was updated
    assert!(!test_cache.as_ref().unwrap().is_stale(MCP_CACHE_TTL_HOURS));
    assert_eq!(test_cache.as_ref().unwrap().data.servers.len(), 2); // Now has both servers
}

#[test]
fn test_registry_error_categorization() {
    // Test that registry errors are properly categorized for sync behavior

    // Network connectivity errors
    let network_errors = [
        "connection refused",
        "timeout",
        "dns resolution failed",
        "network unreachable",
        "http 404",
        "http 503",
    ];

    for error_msg in network_errors {
        let error = eyre::eyre!(error_msg);
        let error_type = RegistryErrorType::from_error(&error);
        assert!(matches!(error_type, RegistryErrorType::NetworkConnectivity));
    }

    // Registry data errors
    let data_errors = [
        "json parse error",
        "validation failed",
        "invalid schema",
        "missing required field",
    ];

    for error_msg in data_errors {
        let error = eyre::eyre!(error_msg);
        let error_type = RegistryErrorType::from_error(&error);
        assert!(matches!(error_type, RegistryErrorType::RegistryData));
    }
}

#[test]
fn test_cache_ttl_edge_cases() {
    // Test cache TTL behavior at edge cases (using 5 minutes for testing)
    let now = time::OffsetDateTime::now_utc();

    // Exactly at TTL boundary (24 hours)
    let boundary_cache = CachedRegistry {
        data: McpRegistryResponse { servers: vec![] },
        fetched_at: now - time::Duration::hours(24),
        source_url: "test".to_string(),
    };
    assert!(boundary_cache.is_stale(MCP_CACHE_TTL_HOURS));

    // Just under TTL (23 hours 59 minutes)
    let fresh_cache = CachedRegistry {
        data: McpRegistryResponse { servers: vec![] },
        fetched_at: now - time::Duration::hours(23) - time::Duration::minutes(59),
        source_url: "test".to_string(),
    };
    assert!(!fresh_cache.is_stale(MCP_CACHE_TTL_HOURS));

    // Way over TTL (48 hours)
    let very_stale_cache = CachedRegistry {
        data: McpRegistryResponse { servers: vec![] },
        fetched_at: now - time::Duration::hours(48),
        source_url: "test".to_string(),
    };
    assert!(very_stale_cache.is_stale(MCP_CACHE_TTL_HOURS));
}
