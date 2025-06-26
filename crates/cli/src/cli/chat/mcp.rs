use std::collections::HashMap;
use std::io::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use crossterm::{
    execute,
    style,
};
use eyre::{
    Result,
    bail,
};
use tracing::warn;

use crate::cli::chat::cli::{
    Mcp,
    McpAdd,
    McpImport,
    McpList,
    McpRemove,
    McpStatus,
    McpUseProfileServersOnly,
    Scope,
};
use crate::cli::chat::tool_manager::{
    McpServerConfig,
    global_mcp_config_path,
    profile_mcp_path,
    queue_profile_exclusive_warning,
    workspace_mcp_config_path,
};
use crate::cli::chat::tools::custom_tool::{
    CustomToolConfig,
    default_timeout,
};
use crate::cli::chat::util::shared_writer::SharedWriter;
use crate::platform::Context;

pub async fn execute_mcp(args: Mcp) -> Result<ExitCode> {
    let ctx = Context::new();
    let mut output = SharedWriter::stdout();

    match args {
        Mcp::Add(args) => add_mcp_server(&ctx, &mut output, args).await?,
        Mcp::Remove(args) => remove_mcp_server(&ctx, &mut output, args).await?,
        Mcp::List(args) => list_mcp_server(&ctx, &mut output, args).await?,
        Mcp::Import(args) => import_mcp_server(&ctx, &mut output, args).await?,
        Mcp::Status(args) => get_mcp_server_status(&ctx, &mut output, args).await?,
        Mcp::UseProfileServersOnly(args) => set_profile_servers_only(&ctx, &mut output, args).await?,
    }

    output.flush()?;
    Ok(ExitCode::SUCCESS)
}

pub async fn add_mcp_server(ctx: &Context, output: &mut SharedWriter, args: McpAdd) -> Result<()> {
    let scope = args.scope.unwrap_or(Scope::Workspace);
    let config_path = resolve_scope_profile(ctx, args.scope, args.profile)?;

    let mut config: McpServerConfig = ensure_config_file(ctx, &config_path, output).await?;

    if config.mcp_servers.contains_key(&args.name) && !args.force {
        bail!(
            "\nMCP server '{}' already exists in {} (scope {}). Use --force to overwrite.",
            args.name,
            config_path.display(),
            scope
        );
    }

    let merged_env = args.env.into_iter().flatten().collect::<HashMap<_, _>>();
    let tool: CustomToolConfig = serde_json::from_value(serde_json::json!({
        "command": args.command,
        "env": merged_env,
        "timeout": args.timeout.unwrap_or(default_timeout()),
    }))?;

    writeln!(
        output,
        "\nTo learn more about MCP safety, see https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-mcp-security.html\n\n"
    )?;

    config.mcp_servers.insert(args.name.clone(), tool);
    config.save_to_file(ctx, &config_path).await?;
    writeln!(
        output,
        "✓ Added MCP server '{}' to {}\n",
        args.name,
        scope_display(&scope)
    )?;
    Ok(())
}

pub async fn remove_mcp_server(ctx: &Context, output: &mut SharedWriter, args: McpRemove) -> Result<()> {
    let scope = args.scope.unwrap_or(Scope::Workspace);
    let config_path = resolve_scope_profile(ctx, args.scope, args.profile)?;

    if !ctx.fs().exists(&config_path) {
        writeln!(output, "\nNo MCP server configurations found.\n")?;
        return Ok(());
    }

    let mut config = McpServerConfig::load_from_file(ctx, &config_path).await?;
    match config.mcp_servers.remove(&args.name) {
        Some(_) => {
            config.save_to_file(ctx, &config_path).await?;
            writeln!(
                output,
                "\n✓ Removed MCP server '{}' from {}\n",
                args.name,
                scope_display(&scope)
            )?;
        },
        None => {
            writeln!(
                output,
                "\nNo MCP server named '{}' found in {}\n",
                args.name,
                scope_display(&scope)
            )?;
        },
    }
    Ok(())
}

pub async fn list_mcp_server(ctx: &Context, output: &mut SharedWriter, args: McpList) -> Result<()> {
    let configs = get_mcp_server_configs(ctx, output, args.scope, args.profile).await?;
    if configs.is_empty() {
        writeln!(output, "No MCP server configurations found.\n")?;
        return Ok(());
    }

    for (scope, path, cfg_opt) in configs {
        writeln!(output)?;
        writeln!(output, "{}:\n  {}", scope_display(&scope), path.display())?;
        match cfg_opt {
            Some(cfg) if !cfg.mcp_servers.is_empty() => {
                for (name, tool_cfg) in &cfg.mcp_servers {
                    writeln!(output, "    • {name:<12} {}", tool_cfg.command)?;
                }
            },
            _ => {
                writeln!(output, "    (empty)")?;
            },
        }
    }
    writeln!(output, "\n")?;
    Ok(())
}

pub async fn import_mcp_server(ctx: &Context, output: &mut SharedWriter, args: McpImport) -> Result<()> {
    let scope: Scope = args.scope.unwrap_or(Scope::Workspace);
    let config_path = resolve_scope_profile(ctx, args.scope, args.profile)?;
    let mut dst_cfg = ensure_config_file(ctx, &config_path, output).await?;

    let src_path = expand_path(ctx, &args.file)?;
    let src_cfg: McpServerConfig = McpServerConfig::load_from_file(ctx, &src_path).await?;

    let mut added = 0;
    for (name, cfg) in src_cfg.mcp_servers {
        if dst_cfg.mcp_servers.contains_key(&name) && !args.force {
            bail!(
                "\nMCP server '{}' already exists in {} (scope {}). Use --force to overwrite.\n",
                name,
                config_path.display(),
                scope
            );
        }
        dst_cfg.mcp_servers.insert(name.clone(), cfg);
        added += 1;
    }

    writeln!(
        output,
        "\nTo learn more about MCP safety, see https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-mcp-security.html\n\n"
    )?;

    dst_cfg.save_to_file(ctx, &config_path).await?;
    writeln!(
        output,
        "✓ Imported {added} MCP server(s) into {}\n",
        scope_display(&scope)
    )?;
    Ok(())
}

pub async fn get_mcp_server_status(ctx: &Context, output: &mut SharedWriter, args: McpStatus) -> Result<()> {
    let configs = get_mcp_server_configs(ctx, output, None, args.profile).await?;
    let mut found = false;

    for (sc, path, cfg_opt) in configs {
        if let Some(cfg) = cfg_opt.and_then(|c| c.mcp_servers.get(&args.name).cloned()) {
            found = true;
            execute!(
                output,
                style::Print("\n─────────────\n"),
                style::Print(format!("Scope   : {}\n", scope_display(&sc))),
                style::Print(format!("File    : {}\n", path.display())),
                style::Print(format!("Command : {}\n", cfg.command)),
                style::Print(format!("Timeout : {} ms\n", cfg.timeout)),
                style::Print(format!(
                    "Env Vars: {}\n",
                    cfg.env
                        .as_ref()
                        .map_or_else(|| "(none)".into(), |e| e.keys().cloned().collect::<Vec<_>>().join(", "))
                )),
            )?;
        }
    }
    writeln!(output, "\n")?;

    if !found {
        bail!("No MCP server named '{0}' found in any scope/profile\n", args.name);
    }
    Ok(())
}

pub async fn get_mcp_server_configs(
    ctx: &Context,
    output: &mut SharedWriter,
    scope: Option<Scope>,
    profile: Option<String>,
) -> Result<Vec<(Scope, PathBuf, Option<McpServerConfig>)>> {
    let mut results = Vec::new();

    // Determine which scopes to include based on the scope parameter
    let scopes_to_include = match scope {
        Some(specific_scope) => vec![specific_scope],
        None => vec![Scope::Profile, Scope::Workspace, Scope::Global],
    };

    // Process each scope
    for sc in scopes_to_include {
        // Skip Profile scope if no profile name is provided
        if sc == Scope::Profile && profile.is_none() {
            continue;
        }

        // Resolve the path for this scope using resolve_scope_profile consistently
        let path = resolve_scope_profile(ctx, Some(sc), profile.clone())?;

        // Load the configuration if it exists
        let cfg_opt = if ctx.fs().exists(&path) {
            match McpServerConfig::load_from_file(ctx, &path).await {
                Ok(cfg) => Some(cfg),
                Err(e) => {
                    warn!(?path, error = %e, "Invalid MCP config file—ignored, treated as null");
                    None
                },
            }
        } else {
            None
        };

        // Check if this is a profile with use_profile_servers_only set to true
        if sc == Scope::Profile {
            if let Some(cfg) = &cfg_opt {
                if cfg.use_profile_servers_only {
                    queue_profile_exclusive_warning(output)?;
                    return Ok(vec![(sc, path, cfg_opt)]);
                }
            }
        }

        results.push((sc, path, cfg_opt));
    }

    Ok(results)
}

fn scope_display(scope: &Scope) -> String {
    match scope {
        Scope::Workspace => "📄 workspace".into(),
        Scope::Global => "🌍 global".into(),
        Scope::Profile => "👤 profile".into(),
    }
}

fn resolve_scope_profile(ctx: &Context, scope: Option<Scope>, profile: Option<String>) -> Result<PathBuf> {
    match scope {
        Some(Scope::Profile) => {
            let profile_name = profile.ok_or_else(|| eyre::eyre!("Profile name is required when scope is Profile"))?;
            profile_mcp_path(ctx, &profile_name)
        },
        Some(Scope::Global) => global_mcp_config_path(ctx),
        _ => workspace_mcp_config_path(ctx), // None or Workspace both default to workspace
    }
}

fn expand_path(ctx: &Context, p: &str) -> Result<PathBuf> {
    let p = shellexpand::tilde(p);
    let mut path = PathBuf::from(p.as_ref());
    if path.is_relative() {
        path = ctx.env().current_dir()?.join(path);
    }
    Ok(path)
}

async fn ensure_config_file(ctx: &Context, path: &PathBuf, out: &mut SharedWriter) -> Result<McpServerConfig> {
    if !ctx.fs().exists(path) {
        if let Some(parent) = path.parent() {
            ctx.fs().create_dir_all(parent).await?;
        }
        McpServerConfig::default().save_to_file(ctx, path).await?;
        writeln!(out, "\n📁 Created MCP config in '{}'", path.display())?;
    }

    load_cfg(ctx, path).await
}

async fn load_cfg(ctx: &Context, p: &PathBuf) -> Result<McpServerConfig> {
    Ok(if ctx.fs().exists(p) {
        McpServerConfig::load_from_file(ctx, p).await?
    } else {
        McpServerConfig::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::chat::cli::{
        McpAdd,
        McpRemove,
        McpUseProfileServersOnly,
    };
    use crate::util::directories;

    #[tokio::test]
    async fn test_scope_and_profile_defaults_to_workspace() {
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        let path = resolve_scope_profile(&ctx, None, None).unwrap();
        assert_eq!(
            path.to_str(),
            workspace_mcp_config_path(&ctx).unwrap().to_str(),
            "No scope or profile should default to the workspace path"
        );
    }

    #[tokio::test]
    async fn test_resolve_scope_profile_all_cases() {
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();

        // Test default (None scope, None profile) -> workspace
        let path = resolve_scope_profile(&ctx, None, None).unwrap();
        assert_eq!(path, workspace_mcp_config_path(&ctx).unwrap());

        // Test explicit workspace scope
        let path = resolve_scope_profile(&ctx, Some(Scope::Workspace), None).unwrap();
        assert_eq!(path, workspace_mcp_config_path(&ctx).unwrap());

        // Test global scope
        let path = resolve_scope_profile(&ctx, Some(Scope::Global), None).unwrap();
        assert_eq!(path, global_mcp_config_path(&ctx).unwrap());

        // Test profile scope with profile name
        let path = resolve_scope_profile(&ctx, Some(Scope::Profile), Some("test_profile".to_string())).unwrap();
        assert_eq!(path, profile_mcp_path(&ctx, "test_profile").unwrap());
    }

    #[tokio::test]
    async fn test_resolve_scope_profile_errors() {
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();

        // Test Profile scope without profile name should error
        let result = resolve_scope_profile(&ctx, Some(Scope::Profile), None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Profile name is required"));
    }

    #[tokio::test]
    async fn ensure_file_created_and_loaded() {
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        let mut out = SharedWriter::null();
        let path = workspace_mcp_config_path(&ctx).unwrap();

        let cfg = super::ensure_config_file(&ctx, &path, &mut out).await.unwrap();
        assert!(ctx.fs().exists(&path), "config file should be created");
        assert!(cfg.mcp_servers.is_empty());
    }

    #[tokio::test]
    async fn add_then_remove_cycle() {
        use crate::cli::chat::cli::{
            McpAdd,
            McpRemove,
        };

        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        let mut out = SharedWriter::null();

        // 1. add
        let add_args = McpAdd {
            name: "local".into(),
            command: "echo hi".into(),
            env: vec![],
            timeout: None,
            scope: None,
            profile: None,
            force: false,
        };
        add_mcp_server(&ctx, &mut out, add_args).await.unwrap();
        let cfg_path = workspace_mcp_config_path(&ctx).unwrap();
        let cfg: McpServerConfig =
            serde_json::from_str(&ctx.fs().read_to_string(cfg_path.clone()).await.unwrap()).unwrap();
        assert!(cfg.mcp_servers.len() == 1);

        // 2. remove
        let rm_args = McpRemove {
            name: "local".into(),
            scope: None,
            profile: None,
        };
        remove_mcp_server(&ctx, &mut out, rm_args).await.unwrap();

        let cfg: McpServerConfig = serde_json::from_str(&ctx.fs().read_to_string(cfg_path).await.unwrap()).unwrap();
        assert!(cfg.mcp_servers.is_empty());
    }

    #[tokio::test]
    async fn test_mcp_commands_with_profile() {
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        let mut output = SharedWriter::null();

        // Create a test profile directory
        let profile_dir = directories::chat_profiles_dir(&ctx).unwrap().join("test_profile");
        ctx.fs().create_dir_all(&profile_dir).await.unwrap();

        // Test add command
        let add_args = McpAdd {
            name: "test_server".to_string(),
            command: "test_command".to_string(),
            scope: None,
            profile: Some("test_profile".to_string()),
            env: vec![],
            timeout: None,
            force: false,
        };
        add_mcp_server(&ctx, &mut output, add_args).await.unwrap();

        // Test list command
        let list_args = McpList {
            scope: None,
            profile: Some("test_profile".to_string()),
        };
        list_mcp_server(&ctx, &mut output, list_args).await.unwrap();

        // Test remove command
        let remove_args = McpRemove {
            name: "test_server".to_string(),
            scope: None,
            profile: Some("test_profile".to_string()),
        };
        remove_mcp_server(&ctx, &mut output, remove_args).await.unwrap();

        // Test use-profile-servers-only command
        set_profile_servers_only(&ctx, &mut output, McpUseProfileServersOnly {
            profile: "test_profile".to_string(),
            value: true,
        })
        .await
        .unwrap();

        // Verify the flag was set
        let profile_path = profile_mcp_path(&ctx, "test_profile").unwrap();
        let config = McpServerConfig::load_from_file(&ctx, &profile_path).await.unwrap();
        assert!(config.use_profile_servers_only);

        // Clean up
        ctx.fs().remove_file(&profile_path).await.unwrap();
        ctx.fs().remove_dir_all(&profile_dir).await.unwrap();
    }

    #[tokio::test]
    async fn test_profile_servers_only_flag() {
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        let mut output = SharedWriter::null();

        // Add servers to different scopes
        add_mcp_server(&ctx, &mut output, McpAdd {
            name: "profile_server".to_string(),
            command: "profile_command".to_string(),
            scope: Some(Scope::Profile),
            profile: Some("test_profile".to_string()),
            env: vec![],
            timeout: None,
            force: false,
        })
        .await
        .unwrap();

        add_mcp_server(&ctx, &mut output, McpAdd {
            name: "workspace_server".to_string(),
            command: "workspace_command".to_string(),
            scope: Some(Scope::Workspace),
            profile: None,
            env: vec![],
            timeout: None,
            force: false,
        })
        .await
        .unwrap();

        add_mcp_server(&ctx, &mut output, McpAdd {
            name: "global_server".to_string(),
            command: "global_command".to_string(),
            scope: Some(Scope::Global),
            profile: None,
            env: vec![],
            timeout: None,
            force: false,
        })
        .await
        .unwrap();

        // Test with use_profile_servers_only = false (default)
        let configs = get_mcp_server_configs(&ctx, &mut output, None, Some("test_profile".to_string()))
            .await
            .unwrap();
        assert_eq!(
            configs.len(),
            3,
            "Should return configs from all scopes when use_profile_servers_only is false"
        );

        // Verify all scopes are included
        let scopes: Vec<Scope> = configs.iter().map(|(scope, _, _)| *scope).collect();
        assert!(scopes.contains(&Scope::Profile), "Should include Profile scope");
        assert!(scopes.contains(&Scope::Workspace), "Should include Workspace scope");
        assert!(scopes.contains(&Scope::Global), "Should include Global scope");

        // Set use_profile_servers_only = true
        set_profile_servers_only(&ctx, &mut output, McpUseProfileServersOnly {
            profile: "test_profile".to_string(),
            value: true,
        })
        .await
        .unwrap();

        // Test with use_profile_servers_only = true
        let configs = get_mcp_server_configs(&ctx, &mut output, None, Some("test_profile".to_string()))
            .await
            .unwrap();
        assert_eq!(
            configs.len(),
            1,
            "Should return only profile config when use_profile_servers_only is true"
        );
        assert_eq!(configs[0].0, Scope::Profile, "Should return profile scope only");

        // Verify the profile config contains the expected server
        if let Some(ref config) = configs[0].2 {
            assert!(
                config.mcp_servers.contains_key("profile_server"),
                "Profile server should be present"
            );
            assert!(config.use_profile_servers_only, "Flag should be set to true");
        } else {
            panic!("Profile config should exist");
        }
    }
}

pub async fn set_profile_servers_only(
    ctx: &Context,
    output: &mut SharedWriter,
    args: McpUseProfileServersOnly,
) -> Result<()> {
    // Get the profile MCP path
    let profile_mcp_path = profile_mcp_path(ctx, &args.profile)?;

    // Check if the profile exists
    let profile_dir = profile_mcp_path
        .parent()
        .ok_or_else(|| eyre::eyre!("Invalid profile path"))?;
    if !ctx.fs().exists(profile_dir) {
        bail!("Profile '{}' does not exist", args.profile);
    }

    // Load or create the profile MCP configuration
    let mut config = if ctx.fs().exists(&profile_mcp_path) {
        match McpServerConfig::load_from_file(ctx, &profile_mcp_path).await {
            Ok(config) => config,
            Err(e) => {
                warn!("Failed to load profile MCP config: {}", e);
                McpServerConfig::default()
            },
        }
    } else {
        McpServerConfig::default()
    };

    // Set the exclusivity flag
    config.use_profile_servers_only = args.value;

    // Save the configuration
    if let Some(parent) = profile_mcp_path.parent() {
        ctx.fs().create_dir_all(parent).await?;
    }
    config.save_to_file(ctx, &profile_mcp_path).await?;

    writeln!(
        output,
        "\n✓ Set profile '{}' to {} use profile-specific MCP servers exclusively\n",
        args.profile,
        if args.value { "now" } else { "no longer" }
    )?;

    Ok(())
}
