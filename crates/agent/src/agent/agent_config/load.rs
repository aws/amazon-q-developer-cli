//! Agent loading logic with path resolution.

use std::path::{
    Path,
    PathBuf,
};

use tracing::{
    error,
    info,
    warn,
};

use super::definitions::{
    AgentConfig,
    AgentConfigV2025_08_22,
};
use super::types::ResourcePath;
use super::{
    AgentConfigError,
    ConfigSource,
    LoadedAgentConfig,
};
use crate::agent::consts::{
    DEFAULT_AGENT_NAME,
    DEFAULT_AGENT_RESOURCES,
};
use crate::agent::util::providers::SystemProvider;

/// Load all agent configs from workspace and global directories.
pub async fn load_agents(
    system: &dyn SystemProvider,
) -> Result<(Vec<LoadedAgentConfig>, Vec<AgentConfigError>), AgentConfigError> {
    let mut agent_configs = Vec::new();
    let mut errors = Vec::new();

    // Load workspace agents
    if let Some(workspace_agents_dir) = resolve_workspace_agents_dir(system) {
        match load_agents_from_dir(&workspace_agents_dir, ConfigSource::Workspace {
            path: workspace_agents_dir.clone(),
        })
        .await
        {
            Ok((mut valid, mut invalid)) => {
                agent_configs.append(&mut valid);
                errors.append(&mut invalid);
            },
            Err(e) => {
                error!(?e, "failed to read workspace agents");
            },
        }
    }

    // Load global agents
    if let Some(global_agents_dir) = resolve_global_agents_dir(system) {
        match load_agents_from_dir(&global_agents_dir, ConfigSource::Global {
            path: global_agents_dir.clone(),
        })
        .await
        {
            Ok((mut valid, mut invalid)) => {
                agent_configs.append(&mut valid);
                errors.append(&mut invalid);
            },
            Err(e) => {
                error!(?e, "failed to read global agents");
            },
        }
    }

    // Add default agent
    agent_configs.push(build_default_agent(system));

    info!(?agent_configs, "loaded agent configs");

    Ok((agent_configs, errors))
}

fn resolve_workspace_agents_dir(system: &dyn SystemProvider) -> Option<PathBuf> {
    let cwd = system.cwd().ok()?;
    let kiro_path = cwd.join(".kiro").join("agents");
    if kiro_path.exists() {
        return Some(kiro_path);
    }
    let amazonq_path = cwd.join(".amazonq").join("cli-agents");
    if amazonq_path.exists() {
        return Some(amazonq_path);
    }
    None
}

fn resolve_global_agents_dir(system: &dyn SystemProvider) -> Option<PathBuf> {
    let home = system.home()?;
    let kiro_path = home.join(".kiro").join("agents");
    if kiro_path.exists() {
        return Some(kiro_path);
    }
    let amazonq_path = home.join(".aws").join("amazonq").join("cli-agents");
    if amazonq_path.exists() {
        return Some(amazonq_path);
    }
    None
}

pub fn build_default_agent(system: &dyn SystemProvider) -> LoadedAgentConfig {
    let mut resources: Vec<ResourcePath> = DEFAULT_AGENT_RESOURCES.iter().map(|&s| s.into()).collect();

    // Add global steering if exists
    if let Some(home) = system.home() {
        let global_steering = home.join(".kiro").join("steering");
        if global_steering.exists() {
            resources.push(format!("file://{}/**/*.md", global_steering.display()).into());
        }
    }

    // Add workspace steering if exists
    if let Ok(cwd) = system.cwd() {
        let workspace_steering = cwd.join(".kiro").join("steering");
        if workspace_steering.exists() {
            resources.push(format!("file://{}/**/*.md", workspace_steering.display()).into());
        }

        if cwd.join("AmazonQ.md").exists() {
            resources.push("file://AmazonQ.md".into());
        }

        // Add rules pattern if .amazonq exists but .kiro doesn't
        let amazonq_dir = cwd.join(".amazonq");
        let kiro_dir = cwd.join(".kiro");
        if amazonq_dir.exists() && !kiro_dir.exists() {
            resources.push("file://.amazonq/rules/**/*.md".into());
        }
    }

    let config = AgentConfigV2025_08_22 {
        name: DEFAULT_AGENT_NAME.to_string(),
        description: Some("The default agent for Kiro CLI".to_string()),
        system_prompt: Some(include_str!("default_agent_prompt.md").to_string()),
        tools: vec!["*".to_string()],
        use_legacy_mcp_json: true,
        resources,
        ..Default::default()
    };

    LoadedAgentConfig::new(AgentConfig::V2025_08_22(config), ConfigSource::BuiltIn)
}

async fn load_agents_from_dir(
    dir: &Path,
    source: ConfigSource,
) -> Result<(Vec<LoadedAgentConfig>, Vec<AgentConfigError>), AgentConfigError> {
    let mut read_dir = match tokio::fs::read_dir(dir).await {
        Ok(rd) => rd,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Vec::new(), Vec::new()));
        },
        Err(e) => {
            return Err(AgentConfigError::Custom(format!(
                "failed to read agents directory {:?}: {}",
                dir, e
            )));
        },
    };

    let mut agents = Vec::new();
    let mut invalid_agents = Vec::new();

    loop {
        match read_dir.next_entry().await {
            Ok(Some(entry)) => {
                let entry_path = entry.path();

                if entry_path.extension().is_none_or(|ext| ext != "json") {
                    continue;
                }

                let Ok(md) = entry.metadata().await.map_err(|e| {
                    error!(?e, "failed to read metadata for {:?}", entry_path);
                }) else {
                    continue;
                };

                if !md.is_file() {
                    warn!("skipping agent for path {:?}: not a file", entry_path);
                    continue;
                }

                let Ok(entry_contents) = tokio::fs::read_to_string(&entry_path).await.map_err(|e| {
                    error!(?e, "failed to read agent config at {:?}", entry_path);
                }) else {
                    continue;
                };

                match serde_json::from_str(&entry_contents) {
                    Ok(config) => {
                        let source = match &source {
                            ConfigSource::Workspace { .. } => ConfigSource::Workspace {
                                path: entry_path.clone(),
                            },
                            ConfigSource::Global { .. } => ConfigSource::Global {
                                path: entry_path.clone(),
                            },
                            ConfigSource::BuiltIn => ConfigSource::BuiltIn,
                        };
                        agents.push(LoadedAgentConfig::new(config, source));
                    },
                    Err(e) => {
                        invalid_agents.push(AgentConfigError::InvalidAgentConfig {
                            path: entry_path.to_string_lossy().to_string(),
                            message: e.to_string(),
                        });
                    },
                }
            },
            Ok(None) => break,
            Err(e) => {
                error!(?e, "failed to read directory entry in {:?}", dir);
                break;
            },
        }
    }

    Ok((agents, invalid_agents))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::util::test::TestBase;

    #[tokio::test]
    async fn test_build_default_agent_with_steering() {
        let base = TestBase::new()
            .await
            .with_file(("~/.kiro/steering/global.md", "# Global steering"))
            .await
            .with_file((".kiro/steering/local.md", "# Local steering"))
            .await;

        let config = build_default_agent(base.provider());
        let resources: Vec<&str> = config.resources().iter().map(|r| r.as_ref()).collect();

        assert!(resources.iter().any(|r| r.contains("steering")));
    }

    #[tokio::test]
    async fn test_resolve_workspace_agents_dir_prefers_kiro() {
        let base = TestBase::new()
            .await
            .with_file((".kiro/agents/.gitkeep", ""))
            .await
            .with_file((".amazonq/cli-agents/.gitkeep", ""))
            .await;

        let dir = resolve_workspace_agents_dir(base.provider());
        assert!(dir.unwrap().ends_with(".kiro/agents"));
    }

    #[tokio::test]
    async fn test_rules_only_when_amazonq_without_kiro() {
        let base = TestBase::new()
            .await
            .with_file((".amazonq/rules/test.md", "# Rules"))
            .await;

        let config = build_default_agent(base.provider());
        let resources: Vec<&str> = config.resources().iter().map(|r| r.as_ref()).collect();

        assert!(resources.iter().any(|r| r.contains(".amazonq/rules")));
    }

    #[tokio::test]
    async fn test_no_rules_when_kiro_exists() {
        let base = TestBase::new()
            .await
            .with_file((".kiro/.gitkeep", ""))
            .await
            .with_file((".amazonq/rules/test.md", "# Rules"))
            .await;

        let config = build_default_agent(base.provider());
        let resources: Vec<&str> = config.resources().iter().map(|r| r.as_ref()).collect();

        assert!(!resources.iter().any(|r| r.contains(".amazonq/rules")));
    }

    #[tokio::test]
    async fn test_load_agents_from_workspace_and_global() {
        let global_agent = r#"{"name": "global-agent", "tools": ["*"]}"#;
        let workspace_agent = r#"{"name": "workspace-agent", "tools": ["fs_read"]}"#;
        let backup_agent = r#"{"name": "backup-agent", "tools": ["*"]}"#;

        let base = TestBase::new()
            .await
            .with_file(("~/.kiro/agents/global.json", global_agent))
            .await
            .with_file((".kiro/agents/workspace.json", workspace_agent))
            .await
            .with_file(("~/.kiro/agents/backup.json.bak", backup_agent))
            .await;

        let (agents, errors) = load_agents(base.provider()).await.unwrap();

        assert!(errors.is_empty(), "expected no errors, got: {:?}", errors);
        assert_eq!(agents.len(), 3, "expected 3 agents (workspace, global, default)");

        let names: Vec<&str> = agents.iter().map(|a| a.name()).collect();
        assert!(names.contains(&"workspace-agent"));
        assert!(names.contains(&"global-agent"));
        assert!(names.contains(&DEFAULT_AGENT_NAME));
        assert!(!names.contains(&"backup-agent"), "should not load .json.bak files");
    }
}
