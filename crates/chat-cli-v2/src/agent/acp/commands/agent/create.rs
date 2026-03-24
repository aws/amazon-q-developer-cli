//! /agent create subcommand execution

use std::path::{
    Path,
    PathBuf,
};

use agent::agent_config::definitions::{
    AgentConfig,
    AgentConfigV2025_08_22,
};
use agent::agent_config::{
    ConfigSource,
    LoadedAgentConfig,
};
use agent::tui_commands::CommandResult;

use super::super::CommandContext;
use crate::os::{
    Fs,
    Os,
};
use crate::util::paths::PathResolver;

/// Arguments for /agent create subcommand
#[derive(Debug, Clone, Default)]
pub struct AgentCreateArgs {
    pub name: Option<String>,
    pub from: Option<String>,
    pub directory: Option<String>,
}

/// Parse create subcommand args from the remaining string after "create".
/// Supports: `<name>`, `<name> --from <agent>`, `<name> --directory <path>`
pub fn parse_args(args: &str) -> AgentCreateArgs {
    let mut result = AgentCreateArgs::default();
    let parts: Vec<&str> = args.split_whitespace().collect();
    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            "--from" | "-f" => {
                if i + 1 < parts.len() {
                    result.from = Some(parts[i + 1].to_string());
                    i += 2;
                } else {
                    i += 1;
                }
            },
            "--directory" | "-d" => {
                if i + 1 < parts.len() {
                    result.directory = Some(parts[i + 1].to_string());
                    i += 2;
                } else {
                    i += 1;
                }
            },
            _ => {
                if result.name.is_none() {
                    result.name = Some(parts[i].to_string());
                }
                i += 1;
            },
        }
    }
    result
}

/// Expand `~` to the home directory. Slash commands don't get shell expansion,
/// so we handle it explicitly.
fn expand_tilde(path: &str, home_dir: Option<PathBuf>) -> String {
    if (path.starts_with("~/") || path == "~")
        && let Some(home) = home_dir
    {
        return home
            .join(path.strip_prefix("~/").unwrap_or(""))
            .to_string_lossy()
            .to_string();
    }
    path.to_string()
}

/// Special directory keywords for agent creation.
const DIR_GLOBAL: &str = "global";
const DIR_WORKSPACE: &str = "workspace";

/// Resolve the target directory for agent creation.
fn resolve_directory(directory: Option<&str>, os: &Os, cwd: &Path) -> Result<PathBuf, String> {
    let resolver = PathResolver::new(os);
    match directory {
        None | Some(DIR_GLOBAL) => resolver
            .global()
            .agents_dir_for_create()
            .map_err(|e| format!("Could not resolve global agents directory: {}", e)),
        Some(DIR_WORKSPACE) => resolver
            .workspace()
            .agents_dir_for_create()
            .map_err(|e| format!("Could not resolve workspace agents directory: {}", e)),
        Some(dir) => {
            let expanded = expand_tilde(dir, os.env.home());
            let mut path = PathBuf::from(expanded);
            if path.is_relative() {
                path = cwd.join(path);
            }
            if path.exists() && !path.is_dir() {
                return Err("Path must be a directory".to_string());
            }
            Ok(path)
        },
    }
}

/// Core create logic, uses `Fs` for filesystem operations (testable via `Fs::Chroot`/`Fs::Fake`).
pub async fn create_agent(
    args: &AgentCreateArgs,
    agent_configs: &[LoadedAgentConfig],
    os: &Os,
    cwd: &Path,
    fs: &Fs,
) -> CommandResult {
    let name = match &args.name {
        Some(n) => n.clone(),
        None => {
            return CommandResult::error(
                "Agent name is required. Usage: /agent create <name> [--from <agent>] [--directory <path>]",
            );
        },
    };

    let dir = match resolve_directory(args.directory.as_deref(), os, cwd) {
        Ok(d) => d,
        Err(e) => return CommandResult::error(format!("Failed to create agent: {}", e)),
    };

    let file_path = dir.join(format!("{name}.json"));

    // Check for duplicate: same name in the same directory
    if let Some(existing) = agent_configs.iter().find(|c| c.name() == name) {
        let existing_in_same_dir = match existing.source() {
            ConfigSource::Workspace { path } | ConfigSource::Global { path } => path.parent().is_some_and(|p| p == dir),
            _ => false,
        };
        if existing_in_same_dir {
            return CommandResult::error(format!("Agent with name {name} already exists. Aborting"));
        }
    }

    // Build agent config content
    let content = if let Some(from_name) = &args.from {
        let source_config = match agent_configs.iter().find(|c| c.name() == from_name) {
            Some(c) => c,
            None => return CommandResult::error(format!("No agent with name '{}' found", from_name)),
        };
        let mut config = source_config.config().clone();
        match &mut config {
            AgentConfig::V2025_08_22(c) => c.name = name.clone(),
        }
        serde_json::to_string_pretty(&config).unwrap_or_default()
    } else {
        let config = AgentConfig::V2025_08_22(AgentConfigV2025_08_22 {
            name: name.clone(),
            description: Some(String::new()),
            ..Default::default()
        });
        serde_json::to_string_pretty(&config).unwrap_or_default()
    };

    // Create directory if needed and write the file
    if !fs.exists(&dir)
        && let Err(e) = fs.create_dir_all(&dir).await
    {
        return CommandResult::error(format!("Failed to create directory {}: {}", dir.display(), e));
    }

    if fs.exists(&file_path) {
        return CommandResult::error(format!("File already exists at {}. Aborting", file_path.display()));
    }

    if let Err(e) = fs.write(&file_path, content).await {
        return CommandResult::error(format!("Failed to write agent config: {}", e));
    }

    CommandResult::success_with_data(
        format!("Agent '{}' created at {}", name, file_path.display()),
        serde_json::json!({
            "name": name,
            "path": file_path.to_string_lossy(),
        }),
    )
}

/// Entry point called from the agent command handler.
pub async fn execute(args: &AgentCreateArgs, ctx: &CommandContext<'_>) -> CommandResult {
    create_agent(args, ctx.agent_configs, ctx.os, ctx.cwd, &ctx.os.fs).await
}

#[cfg(test)]
mod tests {
    use agent::agent_config::ResolvedGlobalPrompt;

    use super::*;

    async fn test_os() -> Os {
        Os::new().await.unwrap()
    }

    #[test]
    fn test_parse_args_name_only() {
        let args = parse_args("myagent");
        assert_eq!(args.name, Some("myagent".to_string()));
        assert_eq!(args.from, None);
        assert_eq!(args.directory, None);
    }

    #[test]
    fn test_parse_args_with_from() {
        let args = parse_args("myagent --from base-agent");
        assert_eq!(args.name, Some("myagent".to_string()));
        assert_eq!(args.from, Some("base-agent".to_string()));
    }

    #[test]
    fn test_parse_args_with_short_flags() {
        let args = parse_args("myagent -f base -d /tmp");
        assert_eq!(args.from, Some("base".to_string()));
        assert_eq!(args.directory, Some("/tmp".to_string()));
    }

    #[test]
    fn test_parse_args_empty() {
        let args = parse_args("");
        assert_eq!(args.name, None);
    }

    #[test]
    fn test_expand_tilde_with_home() {
        let home = Some(PathBuf::from("/home/user"));
        assert_eq!(expand_tilde("~/agents", home.clone()), "/home/user/agents");
        assert_eq!(expand_tilde("~", home), "/home/user/");
    }

    #[test]
    fn test_expand_tilde_no_home() {
        assert_eq!(expand_tilde("~/agents", None), "~/agents");
    }

    #[test]
    fn test_expand_tilde_absolute_path() {
        assert_eq!(
            expand_tilde("/tmp/agents", Some(PathBuf::from("/home/user"))),
            "/tmp/agents"
        );
    }

    #[tokio::test]
    async fn test_resolve_directory_default() {
        let os = test_os().await;
        let dir = resolve_directory(None, &os, Path::new("/ws")).unwrap();
        assert!(dir.ends_with(".kiro/agents"));
    }

    #[tokio::test]
    async fn test_resolve_directory_workspace() {
        let os = test_os().await;
        let cwd = os.env.current_dir().unwrap();
        let dir = resolve_directory(Some(DIR_WORKSPACE), &os, &cwd).unwrap();
        assert_eq!(dir, cwd.join(".kiro/agents"));
    }

    #[tokio::test]
    async fn test_resolve_directory_global_explicit() {
        let os = test_os().await;
        let dir = resolve_directory(Some(DIR_GLOBAL), &os, Path::new("/ws")).unwrap();
        assert!(dir.ends_with(".kiro/agents"));
    }

    #[tokio::test]
    async fn test_create_agent_success() {
        let os = test_os().await;
        let agents_dir = PathResolver::new(&os).global().agents_dir_for_create().unwrap();
        os.fs.create_dir_all(&agents_dir).await.unwrap();

        let args = AgentCreateArgs {
            name: Some("test-agent".to_string()),
            ..Default::default()
        };
        let result = create_agent(&args, &[], &os, Path::new("/ws"), &os.fs).await;
        assert!(result.success, "expected success: {}", result.message);

        let written = os.fs.read(&agents_dir.join("test-agent.json")).await.unwrap();
        let parsed: AgentConfig = serde_json::from_slice(&written).unwrap();
        assert_eq!(parsed.name(), "test-agent");
    }

    #[tokio::test]
    async fn test_create_agent_no_name() {
        let os = test_os().await;
        let args = AgentCreateArgs::default();
        let result = create_agent(&args, &[], &os, Path::new("/ws"), &os.fs).await;
        assert!(!result.success);
        assert!(result.message.contains("name is required"));
    }

    #[tokio::test]
    async fn test_create_agent_duplicate() {
        let os = test_os().await;
        let agents_dir = PathResolver::new(&os).global().agents_dir_for_create().unwrap();
        os.fs.create_dir_all(&agents_dir).await.unwrap();

        let mut config = AgentConfig::default();
        match &mut config {
            AgentConfig::V2025_08_22(c) => c.name = "existing".to_string(),
        }
        let existing = LoadedAgentConfig::new(
            config,
            ConfigSource::Global {
                path: agents_dir.join("existing.json"),
            },
            ResolvedGlobalPrompt::None,
        );

        let args = AgentCreateArgs {
            name: Some("existing".to_string()),
            ..Default::default()
        };
        let result = create_agent(&args, &[existing], &os, Path::new("/ws"), &os.fs).await;
        assert!(!result.success);
        assert!(result.message.contains("already exists"));
    }

    #[tokio::test]
    async fn test_create_agent_creates_directory() {
        let os = test_os().await;
        let agents_dir = PathResolver::new(&os).global().agents_dir_for_create().unwrap();
        let args = AgentCreateArgs {
            name: Some("new-agent".to_string()),
            ..Default::default()
        };
        let result = create_agent(&args, &[], &os, Path::new("/ws"), &os.fs).await;
        assert!(result.success, "expected success: {}", result.message);
        assert!(os.fs.exists(&agents_dir));
    }

    #[tokio::test]
    async fn test_create_agent_from_existing() {
        let os = test_os().await;
        let agents_dir = PathResolver::new(&os).global().agents_dir_for_create().unwrap();
        os.fs.create_dir_all(&agents_dir).await.unwrap();

        let mut source_config = AgentConfig::default();
        match &mut source_config {
            AgentConfig::V2025_08_22(c) => {
                c.name = "base".to_string();
                c.description = Some("base agent".to_string());
            },
        }
        let source = LoadedAgentConfig::new(
            source_config,
            ConfigSource::Global {
                path: agents_dir.join("base.json"),
            },
            ResolvedGlobalPrompt::None,
        );

        let args = AgentCreateArgs {
            name: Some("copy".to_string()),
            from: Some("base".to_string()),
            ..Default::default()
        };
        let result = create_agent(&args, &[source], &os, Path::new("/ws"), &os.fs).await;
        assert!(result.success, "expected success: {}", result.message);

        let written = os.fs.read(&agents_dir.join("copy.json")).await.unwrap();
        let parsed: AgentConfig = serde_json::from_slice(&written).unwrap();
        assert_eq!(parsed.name(), "copy");
        assert_eq!(parsed.description(), Some("base agent"));
    }
}
