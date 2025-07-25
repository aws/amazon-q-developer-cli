use std::collections::HashSet;

/// Manages session-only server state changes that don't persist across application restarts.
/// This allows users to temporarily enable/disable servers during a chat session without
/// modifying configuration files.
#[derive(Debug, Default, Clone)]
pub struct SessionServerState {
    /// Servers that have been disabled for this session only.
    /// These servers were originally enabled in configuration but have been temporarily disabled.
    disabled_servers: HashSet<String>,
    
    /// Servers that have been enabled for this session only.
    /// These servers were originally disabled in configuration but have been temporarily enabled.
    enabled_servers: HashSet<String>,
}

impl SessionServerState {
    /// Creates a new empty session state.
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Determines if a server should be enabled based on configuration and session overrides.
    /// 
    /// # Arguments
    /// * `server_name` - The name of the server to check
    /// * `config_enabled` - Whether the server is enabled in the configuration file
    /// 
    /// # Returns
    /// `true` if the server should be enabled, `false` otherwise
    /// 
    /// # Logic
    /// - If server is in disabled_servers set: return false (session override)
    /// - If server is in enabled_servers set: return true (session override)
    /// 
    /// Disables a server for this session only.
    /// If the server was previously enabled via session override, that override is removed.
    /// 
    /// # Arguments
    /// * `server_name` - The name of the server to disable
    pub fn disable_server(&mut self, server_name: String) {
        self.disabled_servers.insert(server_name.clone());
        self.enabled_servers.remove(&server_name);
    }
    
    /// Enables a server for this session only.
    /// If the server was previously disabled via session override, that override is removed.
    /// 
    /// # Arguments
    /// * `server_name` - The name of the server to enable
    pub fn enable_server(&mut self, server_name: String) {
        self.enabled_servers.insert(server_name.clone());
        self.disabled_servers.remove(&server_name);
    }
    
    /// Returns whether a server has been disabled for this session.
    /// 
    /// # Arguments
    /// * `server_name` - The name of the server to check
    pub fn is_session_disabled(&self, server_name: &str) -> bool {
        self.disabled_servers.contains(server_name)
    }
    
    /// Returns whether a server has any session-level overrides.
    /// 
    /// # Arguments
    /// * `server_name` - The name of the server to check
    pub fn has_session_override(&self, server_name: &str) -> bool {
        self.disabled_servers.contains(server_name) || self.enabled_servers.contains(server_name)
    }
    
    /// Gets all servers that have been disabled for this session.
    pub fn get_disabled_servers(&self) -> &HashSet<String> {
        &self.disabled_servers
    }
    
    /// Gets all servers that have been enabled for this session.
    pub fn get_enabled_servers(&self) -> &HashSet<String> {
        &self.enabled_servers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_session_state() {
        let state = SessionServerState::new();
        assert!(state.get_disabled_servers().is_empty());
        assert!(state.get_enabled_servers().is_empty());
    }

    #[test]
    fn test_disable_server() {
        let mut state = SessionServerState::new();
        
        state.disable_server("test-server".to_string());
        
        assert!(state.is_session_disabled("test-server"));
        assert!(state.has_session_override("test-server"));
        assert!(state.get_disabled_servers().contains("test-server"));
    }

    #[test]
    fn test_enable_server() {
        let mut state = SessionServerState::new();
        
        state.enable_server("test-server".to_string());
        
        assert!(!state.is_session_disabled("test-server"));
        assert!(state.has_session_override("test-server"));
        assert!(state.get_enabled_servers().contains("test-server"));
    }

    #[test]
    fn test_enable_then_disable() {
        let mut state = SessionServerState::new();
        
        // Enable first
        state.enable_server("test-server".to_string());
        assert!(state.get_enabled_servers().contains("test-server"));
        assert!(!state.is_session_disabled("test-server"));
        
        // Then disable - should remove enable override
        state.disable_server("test-server".to_string());
        assert!(!state.get_enabled_servers().contains("test-server"));
        assert!(state.is_session_disabled("test-server"));
    }

    #[test]
    fn test_disable_then_enable() {
        let mut state = SessionServerState::new();
        
        // Disable first
        state.disable_server("test-server".to_string());
        assert!(state.is_session_disabled("test-server"));
        assert!(state.get_disabled_servers().contains("test-server"));
        
        // Then enable - should remove disable override
        state.enable_server("test-server".to_string());
        assert!(!state.is_session_disabled("test-server"));
        assert!(!state.get_disabled_servers().contains("test-server"));
        assert!(state.get_enabled_servers().contains("test-server"));
    }

    #[test]
    fn test_multiple_servers() {
        let mut state = SessionServerState::new();
        
        state.disable_server("server1".to_string());
        state.enable_server("server2".to_string());
        
        // server1 should be disabled
        assert!(state.is_session_disabled("server1"));
        assert!(state.get_disabled_servers().contains("server1"));
        
        // server2 should be enabled
        assert!(!state.is_session_disabled("server2"));
        assert!(state.get_enabled_servers().contains("server2"));
        
        // Both should have overrides
        assert!(state.has_session_override("server1"));
        assert!(state.has_session_override("server2"));
        
        // server3 should have no overrides
        assert!(!state.has_session_override("server3"));
    }
}
