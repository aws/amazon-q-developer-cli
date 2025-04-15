use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use eyre::Result;
use serde::{
    Deserialize,
    Serialize,
};

/// Represents the permission status for a command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommandPermission {
    /// Command requires confirmation each time
    PerRequest,

    /// Command is trusted and doesn't require confirmation
    Trusted,

    /// Command is blocked and cannot be executed
    Blocked,
}

/// Stores persistent command permissions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandPermissions {
    /// Map of command names to their permission status
    permissions: HashMap<String, CommandPermission>,

    /// Version of the permissions format
    version: u32,
}

impl Default for CommandPermissions {
    fn default() -> Self {
        Self {
            permissions: HashMap::new(),
            version: 1,
        }
    }
}

impl CommandPermissions {
    /// Get the path to the permissions file
    fn get_permissions_path() -> Result<PathBuf> {
        let home_dir = dirs::home_dir().ok_or_else(|| eyre::eyre!("Could not find home directory"))?;
        let config_dir = home_dir.join(".aws").join("amazonq");

        // Create directory if it doesn't exist
        if !config_dir.exists() {
            fs::create_dir_all(&config_dir)?;
        }

        Ok(config_dir.join("command_permissions.json"))
    }

    /// Load permissions from disk
    pub fn load() -> Result<Self> {
        let path = Self::get_permissions_path()?;

        if path.exists() {
            let content = fs::read_to_string(path)?;
            let permissions: CommandPermissions = serde_json::from_str(&content)?;
            Ok(permissions)
        } else {
            Ok(Self::default())
        }
    }

    /// Save permissions to disk
    pub fn save(&self) -> Result<()> {
        let path = Self::get_permissions_path()?;
        let content = serde_json::to_string_pretty(self)?;
        fs::write(path, content)?;
        Ok(())
    }

    /// Check if a command is trusted
    pub fn is_trusted(&self, command: &str) -> bool {
        matches!(self.permissions.get(command), Some(CommandPermission::Trusted))
    }

    /// Check if a command is blocked
    pub fn is_blocked(&self, command: &str) -> bool {
        matches!(self.permissions.get(command), Some(CommandPermission::Blocked))
    }

    /// Set a command as trusted
    pub fn trust_command(&mut self, command: &str) -> Result<()> {
        self.permissions.insert(command.to_string(), CommandPermission::Trusted);
        self.save()
    }

    /// Set a command to require confirmation
    pub fn require_confirmation(&mut self, command: &str) -> Result<()> {
        self.permissions
            .insert(command.to_string(), CommandPermission::PerRequest);
        self.save()
    }

    /// Block a command from being executed
    pub fn block_command(&mut self, command: &str) -> Result<()> {
        self.permissions.insert(command.to_string(), CommandPermission::Blocked);
        self.save()
    }

    /// Reset permissions for a command
    pub fn reset_command(&mut self, command: &str) -> Result<()> {
        self.permissions.remove(command);
        self.save()
    }

    /// Reset all permissions
    pub fn reset_all(&mut self) -> Result<()> {
        self.permissions.clear();
        self.save()
    }

    /// Get all command permissions
    pub fn get_all(&self) -> &HashMap<String, CommandPermission> {
        &self.permissions
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn test_command_permissions() {
        let mut permissions = CommandPermissions::default();

        // Test setting permissions
        permissions
            .permissions
            .insert("test".to_string(), CommandPermission::Trusted);
        permissions
            .permissions
            .insert("test2".to_string(), CommandPermission::PerRequest);
        permissions
            .permissions
            .insert("test3".to_string(), CommandPermission::Blocked);

        // Test checking permissions
        assert!(permissions.is_trusted("test"));
        assert!(!permissions.is_trusted("test2"));
        assert!(!permissions.is_blocked("test"));
        assert!(permissions.is_blocked("test3"));

        // Test resetting permissions
        permissions.permissions.remove("test");
        assert!(!permissions.is_trusted("test"));
    }
}
