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
    pub servers: std::collections::HashMap<String, crate::cli::chat::legacy::custom_tool::CustomToolConfig>,
    pub ignored_servers: Vec<String>,
}

/// Process MCP servers based on registry mode
pub fn process_mcp_servers(
    agent_servers: &std::collections::HashMap<String, crate::cli::chat::legacy::custom_tool::CustomToolConfig>,
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

/// Format a package identifier with a version, avoiding double version tags.
///
/// If the identifier already contains a version specifier (e.g., `@scope/pkg@latest`
/// or `pkg@1.2.3`), the identifier is returned as-is. Otherwise, `@version` is appended.
///
/// For scoped npm packages like `@scope/pkg`, the version tag is the `@` after the
/// package name portion (after the `/`), not the leading `@` of the scope.
fn format_package_identifier(identifier: &str, version: &str) -> String {
    // For scoped packages (@scope/pkg), check for a version tag after the slash
    let has_version = if let Some(slash_pos) = identifier.find('/') {
        // Scoped package: check if there's an @ after the slash (e.g., @scope/pkg@version)
        // SAFETY: slash_pos from find('/') on ASCII '/' is always a valid char boundary
        #[allow(clippy::string_slice)]
        identifier[slash_pos + 1..].contains('@')
    } else {
        // Unscoped package: check if there's an @ anywhere (e.g., pkg@version)
        identifier.contains('@')
    };

    if has_version {
        identifier.to_string()
    } else {
        format!("{identifier}@{version}")
    }
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
    agent_config: &crate::cli::chat::legacy::custom_tool::CustomToolConfig,
) -> Result<crate::cli::chat::legacy::custom_tool::CustomToolConfig> {
    use crate::cli::chat::legacy::custom_tool::CustomToolConfig;

    // Merge OAuth overrides; fall back to default scopes when none are set
    // (empty scopes break Dynamic Client Registration on some servers).
    let oauth_scopes = if !agent_config.oauth_scopes.is_empty() {
        agent_config.oauth_scopes.clone()
    } else if let Some(scopes) = agent_config.oauth.as_ref().and_then(|oc| oc.oauth_scopes.clone()) {
        scopes
    } else {
        crate::cli::chat::legacy::custom_tool::get_default_scopes()
    };

    let mut config = CustomToolConfig {
        transport_type: None, // Will be inferred
        url: String::new(),
        headers: std::collections::HashMap::new(),
        oauth_scopes,
        oauth: agent_config.oauth.clone(),
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

                // Add package identifier with version (skip if identifier already contains a version tag)
                config
                    .args
                    .push(format_package_identifier(&package.identifier, &registry_server.version));

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

                // Add package identifier with version (skip if identifier already contains a version tag)
                config
                    .args
                    .push(format_package_identifier(&package.identifier, &registry_server.version));

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

/// Filter tools and MCP servers in a `LoadedAgentConfig` to only allow servers
/// Adapter that lets an [`McpRegistryResponse`] be used as an
/// [`agent::mcp::McpRegistry`].
///
/// The host wraps a fetched registry response in this adapter and hands it to
/// `Agent::new` (via `AcpSessionBuilder::mcp_registry`). The agent then
/// applies the registry to its config at construction, on every swap, and on
/// every `RefreshMcpRegistry` request — so the host no longer needs to
/// pre-rewrite agent configs before a swap or refresh.
///
/// The adapter also carries `mcp.json` registry-type overrides (servers
/// declared as `{ "type": "registry", "env": {...}, ... }` in workspace or
/// global `mcp.json`). These overrides are read eagerly at construction time
/// (the only async hop the adapter performs) and stored alongside the
/// registry response. At apply time — which must remain sync per the
/// [`McpRegistry`] trait contract — the overrides are injected into
/// `agent_config.mcp_servers` so the existing override-collection logic in
/// [`resolve_registry_servers_for_agent_config`] picks them up.
///
/// Cloning is cheap: both the registry response and the override map are
/// held behind [`std::sync::Arc`]s. `Box<dyn McpRegistry>` of this type is
/// `Clone` via [`dyn_clone`], which lets
/// `SessionManager::handle_refresh_registry` fan a single snapshot out to
/// every active session.
#[derive(Debug, Clone)]
pub struct RegistryAdapter {
    response: std::sync::Arc<McpRegistryResponse>,
    /// `mcp.json` `"type": "registry"` entries indexed by server name. Used
    /// to surface user-supplied env / headers / timeout overrides for
    /// registry-managed servers (e.g. `BRAVE_API_KEY` for `npm-brave-search`).
    /// Empty when no overrides are configured.
    mcp_json_overrides: std::sync::Arc<
        std::collections::HashMap<
            String,
            (
                agent::agent_config::definitions::McpServerConfig,
                agent::agent_config::McpServerConfigSource,
            ),
        >,
    >,
}

impl RegistryAdapter {
    /// Construct an adapter for the given registry response, eagerly loading
    /// `mcp.json` entries whose name is in the registry. File errors are logged
    /// and skipped — a missing or malformed `mcp.json` simply contributes no
    /// overrides, never an error.
    ///
    /// **Filtering rule:** an entry in `mcp.json` is loaded iff its name appears
    /// in the registry. This applies regardless of the entry's declared type
    /// (`Registry`, `Local`, `Remote`) — when registry mode is active, the
    /// registry is the single source of truth for which servers are allowed to
    /// run. A user-defined `Local` server in `mcp.json` whose name isn't in the
    /// registry is dropped. A registry-type entry whose name *is* in the
    /// registry contributes its env / headers / timeout to the resolved config.
    ///
    /// Whether these entries are *applied* to a given agent config is a
    /// decision deferred to [`Self::apply`], which gates injection on
    /// `agent_config.use_legacy_mcp_json` so that agents opting out of legacy
    /// `mcp.json` never see them.
    pub async fn new(
        response: McpRegistryResponse,
        local_mcp_path: Option<&std::path::PathBuf>,
        global_mcp_path: Option<&std::path::PathBuf>,
    ) -> Self {
        use agent::agent_config::definitions::McpServers;

        let registry_server_names: std::collections::HashSet<String> =
            response.servers.iter().map(|e| e.server.name.clone()).collect();

        let mut overrides = std::collections::HashMap::new();
        for (path, source) in [
            local_mcp_path.map(|path| (path, agent::agent_config::McpServerConfigSource::WorkspaceMcpJson)),
            global_mcp_path.map(|path| (path, agent::agent_config::McpServerConfigSource::GlobalMcpJson)),
        ]
        .into_iter()
        .flatten()
        {
            let contents = match tokio::fs::read_to_string(path).await {
                Ok(c) => c,
                Err(_) => continue, // missing files are fine, common case
            };
            // Deserialize into the same structured `McpServers` shape used elsewhere.
            // `McpServerConfig` is an untagged enum, so registry-type entries land in
            // the `Registry(_)` variant automatically. A malformed file fails closed —
            // we log and skip; downstream code treats that as "no overrides from this file".
            let parsed: McpServers = match serde_json::from_str(&contents) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "Failed to parse mcp.json — skipping mcp.json contributions from this file"
                    );
                    continue;
                },
            };
            for (name, config) in parsed.mcp_servers {
                // Only entries whose name is in the registry are kept. Non-registry
                // names are dropped: when registry mode is active, the registry is
                // the source of truth for which servers may load.
                if !registry_server_names.contains(&name) {
                    tracing::debug!(
                        server_name = %name,
                        "mcp.json entry dropped: name not in registry"
                    );
                    continue;
                }
                // First write wins. We process workspace mcp.json before global, so
                // workspace-level entries take priority.
                overrides.entry(name).or_insert((config, source));
            }
        }

        Self {
            response: std::sync::Arc::new(response),
            mcp_json_overrides: std::sync::Arc::new(overrides),
        }
    }
}

impl agent::mcp::McpRegistry for RegistryAdapter {
    fn apply(&self, agent_config: &mut agent::agent_config::LoadedAgentConfig) {
        // 1. Inject mcp.json contributions (already filtered to registry-known names at construction) into
        //    the agent config. Registry-type entries feed `resolve_registry_servers_for_agent_config`'s
        //    override-collection pass; Local/Remote entries pass through as full replacements for the
        //    registry's package definition.
        //
        //    Gated on `use_legacy_mcp_json`: an agent that explicitly opts out of
        //    legacy mcp.json (`"useLegacyMcpJson": false` in its config) opts out
        //    of all mcp.json contributions. The agent-config side wins over
        //    mcp.json for any name collision.
        if agent_config.config().use_legacy_mcp_json() && !self.mcp_json_overrides.is_empty() {
            let existing: std::collections::HashSet<String> =
                agent_config.config().mcp_servers().keys().cloned().collect();
            for (name, (config, source)) in self
                .mcp_json_overrides
                .iter()
                .filter(|(name, _)| !existing.contains(*name))
            {
                agent_config.add_mcp_servers_with_source([(name.clone(), config.clone())], *source);
            }
        }

        // 2. Filter `mcp_servers` and `tools` against the registry. Servers we just injected from mcp.json
        //    survive (they're registry-known by name).
        filter_agent_config_tools_by_registry(agent_config, &self.response);

        // 3. Resolve `Registry(_)` placeholders into concrete `Local` / `Remote` configs, merging in any
        //    per-server overrides (env / headers / timeout) collected during the override-collection pass
        //    at the top of resolve.
        resolve_registry_servers_for_agent_config(agent_config, &self.response);
    }
}

/// Filters the agent config's `mcp_servers` map and `tools` list to those
/// present in the registry. Servers explicitly listed in the agent config but
/// missing from the registry are dropped.
///
/// Also forces `use_legacy_mcp_json = false` so the downstream merge in
/// [`agent::agent_config::LoadedMcpServerConfigs::from_agent_config`] does
/// **not** re-read `mcp.json` — that would let non-registry servers
/// (e.g. a personal `agent-memory` server) bypass governance and load
/// alongside registry-managed ones. Registry-type overrides from `mcp.json`
/// (env / headers / timeout for registry-managed servers) are still
/// honoured because [`RegistryAdapter::apply`] pre-extracts them and
/// injects them into `agent_config.mcp_servers` *before* this function
/// runs.
/// Companion to [`resolve_registry_servers_for_agent_config`].
///
/// Internal: the only caller is [`RegistryAdapter::apply`]. Callers outside
/// this module should go through the [`agent::mcp::McpRegistry`] trait.
fn filter_agent_config_tools_by_registry(
    agent_config: &mut agent::agent_config::LoadedAgentConfig,
    registry: &McpRegistryResponse,
) {
    let registry_servers: std::collections::HashSet<&str> =
        registry.servers.iter().map(|e| e.server.name.as_str()).collect();

    // Remove MCP servers not in the registry
    agent_config.retain_mcp_servers(|name| registry_servers.contains(name));

    // Block downstream `mcp.json` re-merge from re-introducing non-registry
    // servers. RegistryAdapter has already extracted any registry-type
    // overrides we care about and injected them above.
    agent_config.config_mut().set_use_legacy_mcp_json(false);

    let existing_servers: std::collections::HashSet<String> =
        agent_config.config().mcp_servers().keys().cloned().collect();

    let filtered: Vec<String> = agent_config
        .tools()
        .into_iter()
        .filter(|tool| {
            if tool == "*" {
                return true;
            }
            // Only filter MCP server tool references; let everything else through
            match agent::agent_config::parse::ToolNameKind::parse(tool) {
                Ok(
                    agent::agent_config::parse::ToolNameKind::McpFullName { server_name, .. }
                    | agent::agent_config::parse::ToolNameKind::McpServer { server_name }
                    | agent::agent_config::parse::ToolNameKind::McpGlob { server_name, .. },
                ) => existing_servers.contains(server_name) || registry_servers.contains(server_name),
                _ => true,
            }
        })
        .collect();
    agent_config.config_mut().set_tools(filtered);
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
                // SAFETY: slash_pos from find('/'), ASCII char, always valid boundary
                #[allow(clippy::string_slice)]
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

/// Resolve registry servers from a [`McpRegistryResponse`] into concrete
/// [`agent::agent_config::definitions::McpServerConfig`] entries and inject them into the given
/// [`agent::agent_config::LoadedAgentConfig`].
///
/// The agent crate's `McpServerConfig` only has `Local` (stdio) and `Remote` (http) variants, so
/// `"type": "registry"` entries in agent config JSON are silently dropped during deserialization.
/// This function re-materialises those entries by converting each registry server definition into
/// the appropriate concrete variant and adding it via `add_mcp_servers`.
///
/// Only servers that are referenced in the agent's `tools` list (via `@server-name/…` patterns)
/// **and** present in the registry are resolved.
///
/// Internal: the only caller is [`RegistryAdapter::apply`]. Callers outside
/// this module should go through the [`agent::mcp::McpRegistry`] trait.
fn resolve_registry_servers_for_agent_config(
    agent_config: &mut agent::agent_config::LoadedAgentConfig,
    registry: &McpRegistryResponse,
) {
    use agent::agent_config::definitions::{
        LocalMcpServerConfig,
        McpServerConfig as AgentMcpServerConfig,
        RemoteMcpServerConfig,
    };

    // Collect servers that need resolution:
    // 1. Registry-type entries already in mcp_servers (explicit `"type": "registry"`)
    // 2. Servers referenced in tools but missing from mcp_servers (implicit registry)
    let mut servers_to_resolve = std::collections::HashSet::new();

    // Collect overrides from explicit Registry entries (env, headers, timeout)
    let mut overrides: std::collections::HashMap<String, agent::agent_config::definitions::RegistryMcpServerConfig> =
        std::collections::HashMap::new();

    // Find explicit Registry entries in mcp_servers
    for (name, config) in agent_config.config().mcp_servers() {
        if let Some(reg) = config.registry_overrides() {
            overrides.insert(name.clone(), reg.clone());
            servers_to_resolve.insert(name.clone());
        }
    }

    // Find MCP servers referenced in tools but not in mcp_servers
    let existing_servers: std::collections::HashSet<String> =
        agent_config.config().mcp_servers().keys().cloned().collect();
    for tool in &agent_config.tools() {
        let server_name = match agent::agent_config::parse::ToolNameKind::parse(tool) {
            Ok(
                agent::agent_config::parse::ToolNameKind::McpFullName { server_name, .. }
                | agent::agent_config::parse::ToolNameKind::McpServer { server_name }
                | agent::agent_config::parse::ToolNameKind::McpGlob { server_name, .. },
            ) => server_name,
            _ => continue,
        };
        if !existing_servers.contains(server_name) {
            servers_to_resolve.insert(server_name.to_string());
        }
    }

    let mut resolved: Vec<(String, AgentMcpServerConfig)> = Vec::new();

    for server_name in &servers_to_resolve {
        let Some(def) = registry.get_server(server_name) else {
            tracing::debug!(
                "Registry server '{}' referenced in tools but not found in registry, skipping",
                server_name
            );
            continue;
        };

        if !def.remotes.is_empty() {
            let remote = &def.remotes[0];
            let agent_overrides = overrides.get(server_name);
            let mut headers = std::collections::HashMap::new();
            for h in &remote.headers {
                headers.insert(h.name.clone(), h.value.clone());
            }
            // Merge agent header overrides (agent wins)
            if let Some(agent_headers) = agent_overrides.and_then(|o| o.headers.as_ref()) {
                for (k, v) in agent_headers {
                    headers.insert(k.clone(), v.clone());
                }
            }
            let timeout_ms = agent_overrides
                .and_then(|o| o.timeout)
                .unwrap_or_else(agent::agent_config::definitions::default_timeout);

            // Resolve OAuth scopes: oauth.oauthScopes > oauthScopes > default
            // (empty scopes break Dynamic Client Registration on some servers).
            let oauth_scopes = agent_overrides
                .and_then(|o| o.oauth.as_ref())
                .and_then(|oc| oc.oauth_scopes.clone())
                .or_else(|| {
                    agent_overrides
                        .filter(|o| !o.oauth_scopes.is_empty())
                        .map(|o| o.oauth_scopes.clone())
                })
                .unwrap_or_else(agent::agent_config::default_legacy_oauth_scopes);
            let oauth = agent_overrides.and_then(|o| o.oauth.clone());

            resolved.push((
                server_name.clone(),
                AgentMcpServerConfig::Remote(RemoteMcpServerConfig {
                    url: remote.url.clone(),
                    headers,
                    timeout_ms,
                    oauth_scopes,
                    oauth,
                    disabled: false,
                    disabled_tools: Vec::new(),
                    force_auth: false,
                }),
            ));
        } else if !def.packages.is_empty() {
            let package = &def.packages[0];
            let agent_overrides = overrides.get(server_name);
            let (command, args, env) = match package.registry_type.as_str() {
                "npm" => {
                    let mut args = vec!["-y".to_string()];
                    args.extend(package.runtime_arguments.iter().map(|a| a.value.clone()));
                    args.push(format_package_identifier(&package.identifier, &def.version));
                    args.extend(package.package_arguments.iter().map(|a| a.value.clone()));
                    let mut env_map = std::collections::HashMap::new();
                    if let Some(ref url) = package.registry_base_url {
                        env_map.insert("NPM_CONFIG_REGISTRY".to_string(), url.clone());
                    }
                    for ev in &package.environment_variables {
                        env_map.insert(ev.name.clone(), ev.value.clone());
                    }
                    // Merge agent env overrides (agent wins)
                    if let Some(agent_env) = agent_overrides.and_then(|o| o.env.as_ref()) {
                        for (k, v) in agent_env {
                            env_map.insert(k.clone(), v.clone());
                        }
                    }
                    (
                        "npx".to_string(),
                        args,
                        if env_map.is_empty() { None } else { Some(env_map) },
                    )
                },
                "pypi" => {
                    let mut args = Vec::new();
                    if let Some(ref url) = package.registry_base_url {
                        args.push(format!("--default-index={url}"));
                    }
                    args.extend(package.runtime_arguments.iter().map(|a| a.value.clone()));
                    args.push(format_package_identifier(&package.identifier, &def.version));
                    args.extend(package.package_arguments.iter().map(|a| a.value.clone()));
                    let mut env_map = std::collections::HashMap::new();
                    for ev in &package.environment_variables {
                        env_map.insert(ev.name.clone(), ev.value.clone());
                    }
                    // Merge agent env overrides (agent wins)
                    if let Some(agent_env) = agent_overrides.and_then(|o| o.env.as_ref()) {
                        for (k, v) in agent_env {
                            env_map.insert(k.clone(), v.clone());
                        }
                    }
                    (
                        "uvx".to_string(),
                        args,
                        if env_map.is_empty() { None } else { Some(env_map) },
                    )
                },
                "oci" => {
                    let mut args = vec!["run".to_string()];
                    args.extend(package.runtime_arguments.iter().map(|a| a.value.clone()));
                    // Collect all env vars: registry first, then agent overrides
                    let mut all_env = std::collections::HashMap::new();
                    for ev in &package.environment_variables {
                        if !ev.name.trim().is_empty() && !ev.value.trim().is_empty() {
                            all_env.insert(ev.name.clone(), ev.value.clone());
                        }
                    }
                    if let Some(agent_env) = agent_overrides.and_then(|o| o.env.as_ref()) {
                        for (k, v) in agent_env {
                            if !k.trim().is_empty() && !v.trim().is_empty() {
                                all_env.insert(k.clone(), v.clone());
                            }
                        }
                    }
                    for (key, value) in &all_env {
                        args.push("-e".to_string());
                        args.push(format!("{key}={value}"));
                    }
                    let image = if let Some(ref url) = package.registry_base_url {
                        if package.identifier.contains(':') {
                            format!("{}/{}", url, package.identifier)
                        } else {
                            format!("{}/{}:{}", url, package.identifier, def.version)
                        }
                    } else if package.identifier.contains(':') {
                        package.identifier.clone()
                    } else {
                        format!("{}:{}", package.identifier, def.version)
                    };
                    args.push(image);
                    args.extend(package.package_arguments.iter().map(|a| a.value.clone()));
                    ("docker".to_string(), args, None)
                },
                other => {
                    tracing::warn!(
                        "Unknown registry type '{}' for server '{}', skipping",
                        other,
                        server_name
                    );
                    continue;
                },
            };

            let timeout_ms = agent_overrides
                .and_then(|o| o.timeout)
                .unwrap_or_else(agent::agent_config::definitions::default_timeout);
            resolved.push((
                server_name.clone(),
                AgentMcpServerConfig::Local(LocalMcpServerConfig {
                    command,
                    args,
                    env,
                    timeout_ms,
                    disabled: false,
                    disabled_tools: Vec::new(),
                }),
            ));
        }
    }

    if !resolved.is_empty() {
        tracing::debug!(
            "Resolved {} registry servers for agent config: {:?}",
            resolved.len(),
            resolved.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>()
        );
        agent_config.insert_mcp_servers_with_source(resolved, agent::agent_config::McpServerConfigSource::Registry);
    }

    // Remove any Registry placeholders that weren't resolved
    let unresolved: std::collections::HashSet<String> = agent_config
        .config()
        .mcp_servers()
        .iter()
        .filter(|(_, c)| c.is_registry())
        .map(|(n, _)| n.clone())
        .collect();
    if !unresolved.is_empty() {
        tracing::warn!("Removing unresolved registry servers: {:?}", unresolved);
        agent_config.retain_mcp_servers(|name| !unresolved.contains(name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::chat::legacy::custom_tool::CustomToolConfig;

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

    #[test]
    fn test_resolve_registry_servers_for_agent_config_npm() {
        use agent::agent_config::definitions::{
            AgentConfig,
            AgentConfigV2025_08_22,
            McpServerConfig as AgentMcpServerConfig,
        };
        use agent::agent_config::{
            ConfigSource,
            LoadedAgentConfig,
            ResolvedGlobalPrompt,
        };

        let registry_json = r#"{
            "servers": [{
                "server": {
                    "name": "my-npm-server",
                    "description": "An NPM registry server",
                    "version": "2.0.0",
                    "packages": [{
                        "registryType": "npm",
                        "identifier": "@acme/mcp-server",
                        "transport": {"type": "stdio"},
                        "runtimeArguments": [{"type": "positional", "value": "--quiet"}],
                        "packageArguments": [{"type": "positional", "value": "--readonly"}]
                    }]
                }
            }]
        }"#;
        let registry: McpRegistryResponse = serde_json::from_str(registry_json).unwrap();

        let config = AgentConfigV2025_08_22 {
            name: "test-agent".to_string(),
            tools: vec!["@my-npm-server/*".to_string(), "fs_read".to_string()],
            ..Default::default()
        };
        let mut loaded = LoadedAgentConfig::new(
            AgentConfig::V2025_08_22(config),
            ConfigSource::Ephemeral,
            ResolvedGlobalPrompt::None,
        );

        assert!(loaded.config().mcp_servers().is_empty());

        resolve_registry_servers_for_agent_config(&mut loaded, &registry);

        assert_eq!(loaded.config().mcp_servers().len(), 1);
        let server = loaded.config().mcp_servers().get("my-npm-server").unwrap();
        match server {
            AgentMcpServerConfig::Local(local) => {
                assert_eq!(local.command, "npx");
                assert!(local.args.contains(&"-y".to_string()));
                assert!(local.args.contains(&"@acme/mcp-server@2.0.0".to_string()));
                assert!(local.args.contains(&"--quiet".to_string()));
                assert!(local.args.contains(&"--readonly".to_string()));
            },
            AgentMcpServerConfig::Remote(_) | AgentMcpServerConfig::Registry(_) => {
                panic!("Expected Local variant for npm server")
            },
        }
    }

    #[test]
    fn test_resolve_registry_servers_for_agent_config_remote() {
        use agent::agent_config::definitions::{
            AgentConfig,
            AgentConfigV2025_08_22,
            McpServerConfig as AgentMcpServerConfig,
        };
        use agent::agent_config::{
            ConfigSource,
            LoadedAgentConfig,
            ResolvedGlobalPrompt,
        };

        let registry_json = r#"{
            "servers": [{
                "server": {
                    "name": "remote-server",
                    "description": "A remote server",
                    "version": "1.0.0",
                    "remotes": [{"type": "sse", "url": "https://example.com/mcp", "headers": [{"name": "X-Api-Key", "value": "test"}]}]
                }
            }]
        }"#;
        let registry: McpRegistryResponse = serde_json::from_str(registry_json).unwrap();

        let config = AgentConfigV2025_08_22 {
            name: "test-agent".to_string(),
            tools: vec!["@remote-server/*".to_string()],
            ..Default::default()
        };
        let mut loaded = LoadedAgentConfig::new(
            AgentConfig::V2025_08_22(config),
            ConfigSource::Ephemeral,
            ResolvedGlobalPrompt::None,
        );

        resolve_registry_servers_for_agent_config(&mut loaded, &registry);

        let server = loaded.config().mcp_servers().get("remote-server").unwrap();
        match server {
            AgentMcpServerConfig::Remote(remote) => {
                assert_eq!(remote.url, "https://example.com/mcp");
                assert_eq!(remote.headers.get("X-Api-Key").unwrap(), "test");
            },
            AgentMcpServerConfig::Local(_) | AgentMcpServerConfig::Registry(_) => panic!("Expected Remote variant"),
        }
    }

    /// Regression test: `resolve_registry_servers_for_agent_config` must
    /// propagate user-supplied OAuth overrides and fall back to default
    /// scopes (not empty) when none are provided.
    #[test]
    fn test_resolve_registry_servers_preserves_oauth_overrides() {
        use std::collections::HashMap;

        use agent::agent_config::definitions::{
            AgentConfig,
            AgentConfigV2025_08_22,
            McpServerConfig as AgentMcpServerConfig,
            RegistryMcpServerConfig,
        };
        use agent::agent_config::{
            ConfigSource,
            LoadedAgentConfig,
            ResolvedGlobalPrompt,
        };

        let registry_json = r#"{
            "servers": [{
                "server": {
                    "name": "atlassian",
                    "description": "Atlassian Rovo MCP Server",
                    "version": "1.0.0",
                    "remotes": [{"type": "streamable-http", "url": "https://mcp.atlassian.com/v1/mcp"}]
                }
            }]
        }"#;
        let registry: McpRegistryResponse = serde_json::from_str(registry_json).unwrap();

        // Case 1: user provides no OAuth overrides — resolver must fall back to
        // legacy defaults rather than an empty Vec.
        let mut mcp_servers = HashMap::new();
        mcp_servers.insert(
            "atlassian".to_string(),
            AgentMcpServerConfig::Registry(RegistryMcpServerConfig {
                server_type: "registry".to_string(),
                env: None,
                headers: None,
                timeout: None,
                oauth_scopes: Vec::new(),
                oauth: None,
            }),
        );
        let config = AgentConfigV2025_08_22 {
            name: "test-agent".to_string(),
            tools: vec!["@atlassian/*".to_string()],
            mcp_servers,
            ..Default::default()
        };
        let mut loaded = LoadedAgentConfig::new(
            AgentConfig::V2025_08_22(config),
            ConfigSource::Ephemeral,
            ResolvedGlobalPrompt::None,
        );
        resolve_registry_servers_for_agent_config(&mut loaded, &registry);
        let server = loaded.config().mcp_servers().get("atlassian").unwrap();
        match server {
            AgentMcpServerConfig::Remote(remote) => {
                assert_eq!(
                    remote.oauth_scopes,
                    agent::agent_config::default_legacy_oauth_scopes(),
                    "no-override case must fall back to legacy default scopes, not empty Vec"
                );
            },
            other => panic!("Expected Remote variant, got {:?}", other),
        }

        // Case 2: user provides oauth.oauthScopes — resolver must propagate them.
        let mut mcp_servers = HashMap::new();
        mcp_servers.insert(
            "atlassian".to_string(),
            AgentMcpServerConfig::Registry(RegistryMcpServerConfig {
                server_type: "registry".to_string(),
                env: None,
                headers: None,
                timeout: None,
                oauth_scopes: Vec::new(),
                oauth: Some(agent::mcp::oauth_util::OAuthConfig {
                    client_id: Some("custom-client-id".to_string()),
                    client_secret: None,
                    client_metadata_url: None,
                    redirect_uri: None,
                    oauth_scopes: Some(vec!["read:jira-work".to_string(), "write:jira-work".to_string()]),
                }),
            }),
        );
        let config = AgentConfigV2025_08_22 {
            name: "test-agent".to_string(),
            tools: vec!["@atlassian/*".to_string()],
            mcp_servers,
            ..Default::default()
        };
        let mut loaded = LoadedAgentConfig::new(
            AgentConfig::V2025_08_22(config),
            ConfigSource::Ephemeral,
            ResolvedGlobalPrompt::None,
        );
        resolve_registry_servers_for_agent_config(&mut loaded, &registry);
        let server = loaded.config().mcp_servers().get("atlassian").unwrap();
        match server {
            AgentMcpServerConfig::Remote(remote) => {
                assert_eq!(
                    remote.oauth_scopes,
                    vec!["read:jira-work".to_string(), "write:jira-work".to_string()],
                    "user-provided oauth.oauthScopes must be propagated to resolved Remote"
                );
                let oauth = remote.oauth.as_ref().expect("oauth block should be propagated");
                assert_eq!(oauth.client_id.as_deref(), Some("custom-client-id"));
            },
            other => panic!("Expected Remote variant, got {:?}", other),
        }
    }

    #[test]
    fn test_resolve_registry_servers_skips_already_loaded() {
        use std::collections::HashMap;

        use agent::agent_config::definitions::{
            AgentConfig,
            AgentConfigV2025_08_22,
            LocalMcpServerConfig,
            McpServerConfig as AgentMcpServerConfig,
        };
        use agent::agent_config::{
            ConfigSource,
            LoadedAgentConfig,
            ResolvedGlobalPrompt,
        };

        let registry_json = r#"{
            "servers": [{
                "server": {
                    "name": "existing-server",
                    "description": "Test",
                    "version": "1.0.0",
                    "remotes": [{"type": "sse", "url": "https://registry-url.com/mcp"}]
                }
            }]
        }"#;
        let registry: McpRegistryResponse = serde_json::from_str(registry_json).unwrap();

        let mut mcp_servers = HashMap::new();
        mcp_servers.insert(
            "existing-server".to_string(),
            AgentMcpServerConfig::Local(LocalMcpServerConfig {
                command: "node".to_string(),
                args: vec!["server.js".to_string()],
                env: None,
                timeout_ms: 120_000,
                disabled: false,
                disabled_tools: Vec::new(),
            }),
        );

        let config = AgentConfigV2025_08_22 {
            name: "test-agent".to_string(),
            tools: vec!["@existing-server/*".to_string()],
            mcp_servers,
            ..Default::default()
        };
        let mut loaded = LoadedAgentConfig::new(
            AgentConfig::V2025_08_22(config),
            ConfigSource::Ephemeral,
            ResolvedGlobalPrompt::None,
        );

        resolve_registry_servers_for_agent_config(&mut loaded, &registry);

        let server = loaded.config().mcp_servers().get("existing-server").unwrap();
        match server {
            AgentMcpServerConfig::Local(local) => {
                assert_eq!(local.command, "node");
            },
            AgentMcpServerConfig::Remote(_) | AgentMcpServerConfig::Registry(_) => {
                panic!("Should not have overwritten existing Local config")
            },
        }
    }

    #[test]
    fn test_resolve_registry_servers_skips_not_in_registry() {
        use agent::agent_config::definitions::{
            AgentConfig,
            AgentConfigV2025_08_22,
        };
        use agent::agent_config::{
            ConfigSource,
            LoadedAgentConfig,
            ResolvedGlobalPrompt,
        };

        let registry_json = r#"{
            "servers": [{
                "server": {
                    "name": "other-server",
                    "description": "Test",
                    "version": "1.0.0",
                    "remotes": [{"type": "sse", "url": "https://example.com"}]
                }
            }]
        }"#;
        let registry: McpRegistryResponse = serde_json::from_str(registry_json).unwrap();

        let config = AgentConfigV2025_08_22 {
            name: "test-agent".to_string(),
            tools: vec!["@missing-server/*".to_string()],
            ..Default::default()
        };
        let mut loaded = LoadedAgentConfig::new(
            AgentConfig::V2025_08_22(config),
            ConfigSource::Ephemeral,
            ResolvedGlobalPrompt::None,
        );

        resolve_registry_servers_for_agent_config(&mut loaded, &registry);

        assert!(loaded.config().mcp_servers().is_empty());
    }

    #[test]
    fn test_resolve_registry_servers_merges_agent_env_overrides() {
        use agent::agent_config::definitions::{
            AgentConfig,
            AgentConfigV2025_08_22,
            McpServerConfig as AgentMcpServerConfig,
            RegistryMcpServerConfig,
        };
        use agent::agent_config::{
            ConfigSource,
            LoadedAgentConfig,
            ResolvedGlobalPrompt,
        };

        let registry_json = r#"{
            "servers": [{
                "server": {
                    "name": "my-server",
                    "description": "Test server",
                    "version": "1.0.0",
                    "packages": [{
                        "registryType": "npm",
                        "identifier": "@acme/mcp-server",
                        "transport": {"type": "stdio"},
                        "environmentVariables": [
                            {"name": "REGISTRY_VAR", "value": "from-registry"},
                            {"name": "SHARED_VAR", "value": "registry-default"}
                        ]
                    }]
                }
            }]
        }"#;
        let registry: McpRegistryResponse = serde_json::from_str(registry_json).unwrap();

        let mut env_overrides = std::collections::HashMap::new();
        env_overrides.insert("AGENT_VAR".to_string(), "from-agent".to_string());
        env_overrides.insert("SHARED_VAR".to_string(), "agent-override".to_string());

        let mut mcp_servers = std::collections::HashMap::new();
        mcp_servers.insert(
            "my-server".to_string(),
            AgentMcpServerConfig::Registry(RegistryMcpServerConfig {
                server_type: "registry".to_string(),
                env: Some(env_overrides),
                headers: None,
                timeout: Some(45000),
                oauth_scopes: Vec::new(),
                oauth: None,
            }),
        );

        let config = AgentConfigV2025_08_22 {
            name: "test-agent".to_string(),
            tools: vec!["@my-server/*".to_string()],
            mcp_servers,
            ..Default::default()
        };
        let mut loaded = LoadedAgentConfig::new(
            AgentConfig::V2025_08_22(config),
            ConfigSource::Ephemeral,
            ResolvedGlobalPrompt::None,
        );

        resolve_registry_servers_for_agent_config(&mut loaded, &registry);

        let server = loaded.config().mcp_servers().get("my-server").unwrap();
        match server {
            AgentMcpServerConfig::Local(local) => {
                assert_eq!(local.command, "npx");
                let env = local.env.as_ref().unwrap();
                assert_eq!(env.get("REGISTRY_VAR").unwrap(), "from-registry");
                assert_eq!(env.get("SHARED_VAR").unwrap(), "agent-override");
                assert_eq!(env.get("AGENT_VAR").unwrap(), "from-agent");
                assert_eq!(local.timeout_ms, 45000);
            },
            _ => panic!("Expected Local variant after resolution"),
        }
    }

    #[test]
    fn test_resolve_registry_servers_merges_agent_header_overrides_remote() {
        use agent::agent_config::definitions::{
            AgentConfig,
            AgentConfigV2025_08_22,
            McpServerConfig as AgentMcpServerConfig,
            RegistryMcpServerConfig,
        };
        use agent::agent_config::{
            ConfigSource,
            LoadedAgentConfig,
            ResolvedGlobalPrompt,
        };

        let registry_json = r#"{
            "servers": [{
                "server": {
                    "name": "remote-srv",
                    "description": "Remote test",
                    "version": "1.0.0",
                    "remotes": [{"type": "sse", "url": "https://api.example.com/mcp", "headers": [{"name": "X-Registry", "value": "reg-val"}]}]
                }
            }]
        }"#;
        let registry: McpRegistryResponse = serde_json::from_str(registry_json).unwrap();

        let mut header_overrides = std::collections::HashMap::new();
        header_overrides.insert("Authorization".to_string(), "Bearer secret".to_string());
        header_overrides.insert("X-Registry".to_string(), "agent-override".to_string());

        let mut mcp_servers = std::collections::HashMap::new();
        mcp_servers.insert(
            "remote-srv".to_string(),
            AgentMcpServerConfig::Registry(RegistryMcpServerConfig {
                server_type: "registry".to_string(),
                env: None,
                headers: Some(header_overrides),
                timeout: Some(20000),
                oauth_scopes: Vec::new(),
                oauth: None,
            }),
        );

        let config = AgentConfigV2025_08_22 {
            name: "test-agent".to_string(),
            tools: vec!["@remote-srv/*".to_string()],
            mcp_servers,
            ..Default::default()
        };
        let mut loaded = LoadedAgentConfig::new(
            AgentConfig::V2025_08_22(config),
            ConfigSource::Ephemeral,
            ResolvedGlobalPrompt::None,
        );

        resolve_registry_servers_for_agent_config(&mut loaded, &registry);

        let server = loaded.config().mcp_servers().get("remote-srv").unwrap();
        match server {
            AgentMcpServerConfig::Remote(remote) => {
                assert_eq!(remote.url, "https://api.example.com/mcp");
                assert_eq!(remote.headers.get("X-Registry").unwrap(), "agent-override");
                assert_eq!(remote.headers.get("Authorization").unwrap(), "Bearer secret");
                assert_eq!(remote.timeout_ms, 20000);
            },
            _ => panic!("Expected Remote variant after resolution"),
        }
    }

    #[test]
    fn test_filter_agent_config_removes_non_registry_servers() {
        use agent::agent_config::definitions::{
            AgentConfig,
            AgentConfigV2025_08_22,
            LocalMcpServerConfig,
            McpServerConfig as AgentMcpServerConfig,
        };
        use agent::agent_config::{
            ConfigSource,
            LoadedAgentConfig,
            ResolvedGlobalPrompt,
        };

        let registry: McpRegistryResponse = serde_json::from_str(
            r#"{"servers": [{"server": {"name": "approved", "description": "ok", "version": "1.0.0",
                "remotes": [{"type": "sse", "url": "https://example.com"}]}}]}"#,
        )
        .unwrap();

        let mut servers = std::collections::HashMap::new();
        servers.insert(
            "approved".to_string(),
            AgentMcpServerConfig::Local(LocalMcpServerConfig {
                command: "approved-cmd".to_string(),
                args: vec![],
                env: None,
                timeout_ms: 120000,
                disabled: false,
                disabled_tools: vec![],
            }),
        );
        servers.insert(
            "user-local".to_string(),
            AgentMcpServerConfig::Local(LocalMcpServerConfig {
                command: "my-tool".to_string(),
                args: vec![],
                env: None,
                timeout_ms: 120000,
                disabled: false,
                disabled_tools: vec![],
            }),
        );

        let config = AgentConfigV2025_08_22 {
            name: "test".to_string(),
            mcp_servers: servers,
            tools: vec![
                "@approved/tool".to_string(),
                "@user-local/tool".to_string(),
                "read".to_string(),
            ],
            ..Default::default()
        };
        let mut loaded = LoadedAgentConfig::new(
            AgentConfig::V2025_08_22(config),
            ConfigSource::Ephemeral,
            ResolvedGlobalPrompt::None,
        );

        filter_agent_config_tools_by_registry(&mut loaded, &registry);

        // Only "approved" server survives
        assert_eq!(loaded.config().mcp_servers().len(), 1);
        assert!(loaded.config().mcp_servers().contains_key("approved"));
        assert!(!loaded.config().mcp_servers().contains_key("user-local"));

        // Tools filtered accordingly
        let tools = loaded.tools();
        assert!(tools.contains(&"@approved/tool".to_string()));
        assert!(!tools.contains(&"@user-local/tool".to_string()));
        assert!(tools.contains(&"read".to_string()));
    }

    #[test]
    fn test_filter_agent_config_disables_legacy_mcp_json() {
        use agent::agent_config::definitions::{
            AgentConfig,
            AgentConfigV2025_08_22,
        };
        use agent::agent_config::{
            ConfigSource,
            LoadedAgentConfig,
            ResolvedGlobalPrompt,
        };

        let registry: McpRegistryResponse = serde_json::from_str(
            r#"{"servers": [{"server": {"name": "s", "description": "d", "version": "1",
                "remotes": [{"type": "sse", "url": "https://x.com"}]}}]}"#,
        )
        .unwrap();

        let config = AgentConfigV2025_08_22 {
            name: "test".to_string(),
            use_legacy_mcp_json: true,
            ..Default::default()
        };
        let mut loaded = LoadedAgentConfig::new(
            AgentConfig::V2025_08_22(config),
            ConfigSource::Ephemeral,
            ResolvedGlobalPrompt::None,
        );

        assert!(loaded.config().use_legacy_mcp_json());
        filter_agent_config_tools_by_registry(&mut loaded, &registry);
        // When the registry is active, `use_legacy_mcp_json` is forced off so
        // the downstream `mcp.json` merge cannot re-introduce non-registry
        // servers (e.g. a personal `agent-memory` Local server). Registry-type
        // overrides from `mcp.json` are still picked up via
        // `RegistryAdapter::new`'s eager pre-extraction.
        assert!(!loaded.config().use_legacy_mcp_json());
    }

    #[test]
    fn test_filter_agent_config_empty_registry_removes_all_servers() {
        use agent::agent_config::definitions::{
            AgentConfig,
            AgentConfigV2025_08_22,
            LocalMcpServerConfig,
            McpServerConfig as AgentMcpServerConfig,
        };
        use agent::agent_config::{
            ConfigSource,
            LoadedAgentConfig,
            ResolvedGlobalPrompt,
        };

        let empty_registry = McpRegistryResponse { servers: vec![] };

        let mut servers = std::collections::HashMap::new();
        servers.insert(
            "some-server".to_string(),
            AgentMcpServerConfig::Local(LocalMcpServerConfig {
                command: "cmd".to_string(),
                args: vec![],
                env: None,
                timeout_ms: 120000,
                disabled: false,
                disabled_tools: vec![],
            }),
        );

        let config = AgentConfigV2025_08_22 {
            name: "test".to_string(),
            mcp_servers: servers,
            tools: vec!["@some-server/tool".to_string(), "read".to_string()],
            ..Default::default()
        };
        let mut loaded = LoadedAgentConfig::new(
            AgentConfig::V2025_08_22(config),
            ConfigSource::Ephemeral,
            ResolvedGlobalPrompt::None,
        );

        filter_agent_config_tools_by_registry(&mut loaded, &empty_registry);

        assert!(loaded.config().mcp_servers().is_empty());
        let tools = loaded.tools();
        assert!(!tools.contains(&"@some-server/tool".to_string()));
        assert!(tools.contains(&"read".to_string()));
    }

    #[test]
    fn test_format_package_identifier_no_version() {
        // Unscoped package without version → append version
        assert_eq!(format_package_identifier("my-server", "1.0.0"), "my-server@1.0.0");
        // Scoped package without version → append version
        assert_eq!(format_package_identifier("@acme/server", "2.0.0"), "@acme/server@2.0.0");
    }

    #[test]
    fn test_format_package_identifier_with_version() {
        // Unscoped package with version → keep as-is
        assert_eq!(
            format_package_identifier("my-server@latest", "1.0.0"),
            "my-server@latest"
        );
        assert_eq!(format_package_identifier("my-server@1.2.3", "1.0.0"), "my-server@1.2.3");
        // Scoped package with version → keep as-is
        assert_eq!(
            format_package_identifier("@playwright/mcp@latest", "1.0.0"),
            "@playwright/mcp@latest"
        );
        assert_eq!(
            format_package_identifier("@acme/server@0.8.1", "2.0.0"),
            "@acme/server@0.8.1"
        );
    }

    // ===================================================================================
    // RegistryAdapter — filter-by-registry-membership and apply-time gating
    //
    // These tests pin the contract that when registry mode is active:
    //   1. Only mcp.json entries whose name is in the registry survive (any type).
    //   2. Apply-time injection is gated on `useLegacyMcpJson`.
    //   3. Apply forces `use_legacy_mcp_json = false` so the downstream merge in `from_agent_config`
    //      cannot re-introduce filtered-out servers.
    //   4. Registry-type overrides from mcp.json flow into the resolved Local/Remote config (env /
    //      headers / timeout).
    // ===================================================================================

    mod registry_adapter {
        use std::io::Write;

        use agent::agent_config::definitions::{
            AgentConfig,
            AgentConfigV2025_08_22,
            McpServerConfig,
        };
        use agent::agent_config::{
            ConfigSource,
            LoadedAgentConfig,
            ResolvedGlobalPrompt,
        };
        use agent::mcp::McpRegistry;
        use tempfile::TempDir;

        use super::*;

        /// Registry response with two npm-package servers: `npm-brave-search` (in registry)
        /// and `keep-me` (in registry). `agent-memory` is intentionally absent.
        fn fixture_registry() -> McpRegistryResponse {
            serde_json::from_str(
                r#"{"servers":[
                    {"server":{
                        "name":"npm-brave-search",
                        "description":"Brave search MCP",
                        "version":"1.0.0",
                        "packages":[{
                            "registryType":"npm",
                            "identifier":"@brave/brave-search-mcp",
                            "transport":{"type":"stdio"}
                        }]
                    }},
                    {"server":{
                        "name":"keep-me",
                        "description":"Another registry server",
                        "version":"1.0.0",
                        "packages":[{
                            "registryType":"npm",
                            "identifier":"@example/keep",
                            "transport":{"type":"stdio"}
                        }]
                    }}
                ]}"#,
            )
            .unwrap()
        }

        /// Helper to write an `mcp.json` file in a temp dir and return its path.
        fn write_mcp_json(dir: &TempDir, contents: &str) -> std::path::PathBuf {
            let path = dir.path().join("mcp.json");
            let mut f = std::fs::File::create(&path).unwrap();
            f.write_all(contents.as_bytes()).unwrap();
            path
        }

        fn loaded_agent_config(use_legacy: bool, tools: &[&str]) -> LoadedAgentConfig {
            LoadedAgentConfig::new(
                AgentConfig::V2025_08_22(AgentConfigV2025_08_22 {
                    name: "test-agent".to_string(),
                    use_legacy_mcp_json: use_legacy,
                    tools: tools.iter().map(|s| s.to_string()).collect(),
                    ..Default::default()
                }),
                ConfigSource::Ephemeral,
                ResolvedGlobalPrompt::None,
            )
        }

        // -------------------------------------------------------------------------------
        // Construction-time filtering
        // -------------------------------------------------------------------------------

        #[tokio::test]
        async fn drops_non_registry_local_entry() {
            // `agent-memory` (Local) is NOT in the registry → must be dropped at
            // construction. This is the bug we're directly pinning.
            let dir = TempDir::new().unwrap();
            let mcp = write_mcp_json(
                &dir,
                r#"{"mcpServers":{
                    "agent-memory":{"command":"npx","args":["-y","@modelcontextprotocol/server-memory"]}
                }}"#,
            );
            let adapter = RegistryAdapter::new(fixture_registry(), Some(&mcp), None).await;
            assert!(
                !adapter.mcp_json_overrides.contains_key("agent-memory"),
                "non-registry Local entry must be dropped"
            );
        }

        #[tokio::test]
        async fn drops_non_registry_registry_type_entry() {
            // A registry-type entry whose name isn't in the registry is also dropped —
            // otherwise it would survive to launch time and fail with the "not resolved"
            // safety net.
            let dir = TempDir::new().unwrap();
            let mcp = write_mcp_json(
                &dir,
                r#"{"mcpServers":{
                    "ghost-server":{"type":"registry","env":{"X":"y"}}
                }}"#,
            );
            let adapter = RegistryAdapter::new(fixture_registry(), Some(&mcp), None).await;
            assert!(
                !adapter.mcp_json_overrides.contains_key("ghost-server"),
                "registry-type entry for unknown name must be dropped"
            );
        }

        #[tokio::test]
        async fn keeps_registry_type_with_env_override() {
            // The original BRAVE_API_KEY case: registry-type entry whose name IS in the
            // registry → kept, carrying the env override.
            let dir = TempDir::new().unwrap();
            let mcp = write_mcp_json(
                &dir,
                r#"{"mcpServers":{
                    "npm-brave-search":{"type":"registry","env":{"BRAVE_API_KEY":"abc123"}}
                }}"#,
            );
            let adapter = RegistryAdapter::new(fixture_registry(), Some(&mcp), None).await;
            let entry = adapter
                .mcp_json_overrides
                .get("npm-brave-search")
                .expect("registry-type entry with name in registry must be kept");
            match entry {
                (McpServerConfig::Registry(reg), source) => {
                    let env = reg.env.as_ref().expect("env override missing");
                    assert_eq!(env.get("BRAVE_API_KEY").map(String::as_str), Some("abc123"));
                    assert_eq!(*source, agent::agent_config::McpServerConfigSource::WorkspaceMcpJson);
                },
                other => panic!("expected Registry variant, got {:?}", other),
            }
        }

        #[tokio::test]
        async fn keeps_local_with_name_in_registry() {
            // A user-defined Local entry whose name IS in the registry replaces the
            // registry's package definition. The user's command/args take precedence.
            let dir = TempDir::new().unwrap();
            let mcp = write_mcp_json(
                &dir,
                r#"{"mcpServers":{
                    "keep-me":{"command":"my-custom","args":["--mine"]}
                }}"#,
            );
            let adapter = RegistryAdapter::new(fixture_registry(), Some(&mcp), None).await;
            match adapter.mcp_json_overrides.get("keep-me") {
                Some((McpServerConfig::Local(local), source)) => {
                    assert_eq!(local.command, "my-custom");
                    assert_eq!(local.args, vec!["--mine".to_string()]);
                    assert_eq!(*source, agent::agent_config::McpServerConfigSource::WorkspaceMcpJson);
                },
                other => panic!("expected Local variant, got {:?}", other),
            }
        }

        #[tokio::test]
        async fn workspace_wins_over_global() {
            // Both files declare an override for the same registry-known name with
            // different env values. Workspace (passed first) takes priority.
            let workspace_dir = TempDir::new().unwrap();
            let global_dir = TempDir::new().unwrap();
            let workspace = write_mcp_json(
                &workspace_dir,
                r#"{"mcpServers":{
                    "npm-brave-search":{"type":"registry","env":{"BRAVE_API_KEY":"workspace"}}
                }}"#,
            );
            let global = write_mcp_json(
                &global_dir,
                r#"{"mcpServers":{
                    "npm-brave-search":{"type":"registry","env":{"BRAVE_API_KEY":"global"}}
                }}"#,
            );
            let adapter = RegistryAdapter::new(fixture_registry(), Some(&workspace), Some(&global)).await;
            let entry = adapter.mcp_json_overrides.get("npm-brave-search").unwrap();
            match entry {
                (McpServerConfig::Registry(reg), source) => {
                    assert_eq!(
                        reg.env
                            .as_ref()
                            .and_then(|e| e.get("BRAVE_API_KEY"))
                            .map(String::as_str),
                        Some("workspace"),
                        "workspace mcp.json must win over global"
                    );
                    assert_eq!(*source, agent::agent_config::McpServerConfigSource::WorkspaceMcpJson);
                },
                other => panic!("expected Registry variant, got {:?}", other),
            }
        }

        #[tokio::test]
        async fn handles_missing_files() {
            // Paths to non-existent files: no panic, no overrides.
            let bogus = std::path::PathBuf::from("/tmp/definitely-not-a-real-mcp-json-12345.json");
            let adapter = RegistryAdapter::new(fixture_registry(), Some(&bogus), Some(&bogus)).await;
            assert!(adapter.mcp_json_overrides.is_empty());
        }

        #[tokio::test]
        async fn handles_malformed_json() {
            // Garbage file: log + skip, no overrides loaded from it.
            let dir = TempDir::new().unwrap();
            let mcp = write_mcp_json(&dir, "not valid json {{{");
            let adapter = RegistryAdapter::new(fixture_registry(), Some(&mcp), None).await;
            assert!(adapter.mcp_json_overrides.is_empty());
        }

        // -------------------------------------------------------------------------------
        // Apply-time gating
        // -------------------------------------------------------------------------------

        async fn adapter_with_brave_override() -> RegistryAdapter {
            let dir = TempDir::new().unwrap();
            let mcp = write_mcp_json(
                &dir,
                r#"{"mcpServers":{
                    "npm-brave-search":{"type":"registry","env":{"BRAVE_API_KEY":"abc"}}
                }}"#,
            );
            let adapter = RegistryAdapter::new(fixture_registry(), Some(&mcp), None).await;
            // TempDir drops at end of fn, but adapter has already read the contents.
            std::mem::drop(dir);
            adapter
        }

        #[tokio::test]
        async fn apply_injects_when_legacy_enabled() {
            let adapter = adapter_with_brave_override().await;
            let mut config = loaded_agent_config(true, &["@npm-brave-search/web_search"]);
            adapter.apply(&mut config);
            // npm-brave-search must end up in mcp_servers — injected, then resolved by the
            // resolve step into a concrete Local/Remote with the env override carried over.
            let entry = config
                .config()
                .mcp_servers()
                .get("npm-brave-search")
                .expect("registry server should have been injected and resolved");
            match entry {
                McpServerConfig::Local(local) => {
                    let env = local.env.as_ref().expect("env override should have flowed through");
                    assert_eq!(env.get("BRAVE_API_KEY").map(String::as_str), Some("abc"));
                },
                other => panic!("expected resolved Local, got {:?}", other),
            }
            assert_eq!(
                config.mcp_server_source("npm-brave-search"),
                agent::agent_config::McpServerConfigSource::Registry
            );
        }

        #[tokio::test]
        async fn apply_skips_when_legacy_disabled() {
            // Agent config explicitly opts out of legacy mcp.json. The mcp.json override
            // must NOT be injected, even though the adapter has it pre-loaded.
            let adapter = adapter_with_brave_override().await;
            let mut config = loaded_agent_config(false, &["@npm-brave-search/web_search"]);
            adapter.apply(&mut config);
            // No injection happened, so the env override never made it into the resolved
            // config. The server may still resolve from the registry's package def alone
            // (without env), but its env must not be the BRAVE_API_KEY value.
            if let Some(McpServerConfig::Local(local)) = config.config().mcp_servers().get("npm-brave-search") {
                let env_has_brave = local.env.as_ref().and_then(|e| e.get("BRAVE_API_KEY")).is_some();
                assert!(
                    !env_has_brave,
                    "useLegacyMcpJson=false must skip mcp.json env override injection"
                );
            }
        }

        #[tokio::test]
        async fn apply_forces_use_legacy_mcp_json_to_false() {
            // After apply runs in registry mode, the flag is forced off so the downstream
            // merge in `LoadedMcpServerConfigs::from_agent_config` cannot re-introduce
            // mcp.json entries the registry filter just dropped.
            let adapter = adapter_with_brave_override().await;
            let mut config = loaded_agent_config(true, &["@npm-brave-search/web_search"]);
            assert!(config.config().use_legacy_mcp_json());
            adapter.apply(&mut config);
            assert!(
                !config.config().use_legacy_mcp_json(),
                "apply must clear use_legacy_mcp_json so from_agent_config doesn't re-read mcp.json"
            );
        }

        #[tokio::test]
        async fn apply_drops_tool_refs_to_non_registry_servers() {
            // Tools referencing servers that aren't in the registry must be filtered out
            // of the agent's tools list. Registry-listed tool refs survive.
            let dir = TempDir::new().unwrap();
            let mcp = write_mcp_json(
                &dir,
                r#"{"mcpServers":{
                    "agent-memory":{"command":"npx","args":["-y","@modelcontextprotocol/server-memory"]}
                }}"#,
            );
            let adapter = RegistryAdapter::new(fixture_registry(), Some(&mcp), None).await;
            let mut config = loaded_agent_config(true, &["@npm-brave-search/web_search", "@agent-memory/save"]);
            adapter.apply(&mut config);
            let tools = config.config().tools();
            assert!(
                tools.iter().any(|t| t == "@npm-brave-search/web_search"),
                "registry-listed tool ref should survive; got {:?}",
                tools
            );
            assert!(
                !tools.iter().any(|t| t == "@agent-memory/save"),
                "non-registry tool ref must be dropped; got {:?}",
                tools
            );
            assert!(
                !config.config().mcp_servers().contains_key("agent-memory"),
                "non-registry server must not appear in mcp_servers after apply"
            );
        }
    }
    #[tokio::test]
    async fn test_registry_sync_behavior() {
        use std::collections::HashMap;

        use crate::cli::chat::legacy::custom_tool::CustomToolConfig;

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
