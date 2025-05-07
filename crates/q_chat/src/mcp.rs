use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use eyre::{
    Result,
    bail,
};
use fig_os_shim::Context;
use tracing::warn;

use crate::cli::{
    Mcp,
    McpAdd,
    McpImport,
    McpList,
    McpRemove,
    Scope,
};
use crate::tool_manager::{
    McpServerConfig,
    global_mcp_config_path,
    profile_mcp_config_path,
    workspace_mcp_config_path,
};
use crate::tools::custom_tool::CustomToolConfig;

pub async fn execute_mcp(args: Mcp) -> Result<ExitCode> {
    let ctx = Context::new();

    match args {
        Mcp::Add(args) => {
            add_mcp_server(&ctx, args).await?;
        },
        Mcp::Remove(args) => remove_mcp_server(&ctx, args).await?,
        Mcp::List(args) => list_mcp_server(&ctx, args).await?,
        Mcp::Import(args) => import_mcp_server(&ctx, args).await?,
        Mcp::Status { name } => get_mcp_server_status(&ctx, name).await?,
    }

    Ok(ExitCode::SUCCESS)
}

pub async fn add_mcp_server(ctx: &Context, args: McpAdd) -> Result<()> {
    let config_path = resolve_scope_profile(ctx, args.scope, args.profile.as_ref())?;
    let mut config: McpServerConfig = serde_json::from_str(&ctx.fs().read_to_string(&config_path).await?)?;
    let val: CustomToolConfig = serde_json::from_value(serde_json::json!({
        "command": args.command,
        "env": args.env,
        "timeout": args.timeout,
    }))?;
    config.mcp_servers.insert(args.name, val);
    config.save_to_file(ctx, &config_path).await?;

    Ok(())
}

pub async fn remove_mcp_server(ctx: &Context, args: McpRemove) -> Result<()> {
    let config_path = resolve_scope_profile(ctx, args.scope, args.profile.as_ref())?;
    let mut config = McpServerConfig::load_from_file(ctx, &config_path).await?;
    match config.mcp_servers.remove(&args.name) {
        Some(_) => (),
        None => {
            warn!(?args, "No MCP server found");
        },
    }
    Ok(())
}

pub async fn list_mcp_server(ctx: &Context, args: McpList) -> Result<()> {
    let configs = get_mcp_server_configs(ctx, args.scope, args.profile).await?;
    if configs.is_empty() {
        println!("No MCP server configurations found.\n");
        return Ok(());
    }

    for (scope, profile, path, cfg_opt) in configs {
        println!("{}:", scope_display(&scope, &profile));
        println!("  {}", path.display());
        match cfg_opt {
            Some(cfg) if !cfg.mcp_servers.is_empty() => {
                for (name, tool_cfg) in &cfg.mcp_servers {
                    println!("    • {name:<12} {}", tool_cfg.command);
                }
            },
            _ => {
                println!("    null");
            },
        }
    }
    Ok(())
}

async fn get_mcp_server_configs(
    ctx: &Context,
    scope: Option<Scope>,
    profile: Option<String>,
) -> Result<Vec<(Scope, Option<String>, PathBuf, Option<McpServerConfig>)>> {
    let targets: Vec<(Scope, Option<String>)> = match (scope, profile) {
        (Some(Scope::Workspace), _) => vec![(Scope::Workspace, None)],
        (Some(Scope::Global), _) => vec![(Scope::Global, None)],
        (Some(Scope::Profile), Some(p)) => vec![(Scope::Profile, Some(p))],
        (Some(Scope::Profile), None) => vec![(Scope::Profile, Some("default".to_string()))],

        // no scope but have profile ⇒ search profile
        (None, Some(p)) => vec![(Scope::Profile, Some(p))],

        // give nothing ⇒ default priority：workspace → global
        (None, None) => vec![
            (Scope::Workspace, None),
            (Scope::Global, None),
            (Scope::Profile, Some("default".to_string())),
        ],
    };

    let mut results = Vec::new();
    for (sc, prof) in targets {
        let path = resolve_scope_profile(ctx, Some(sc), prof.as_ref())?;

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
        results.push((sc, prof.clone(), path, cfg_opt));
    }
    Ok(results)
}

fn scope_display(scope: &Scope, profile: &Option<String>) -> String {
    match scope {
        Scope::Workspace => "\n📄 workspace".into(),
        Scope::Global => "\n🌍 global".into(),
        Scope::Profile => format!("\n👤 profile({})", profile.as_deref().unwrap_or("default")),
    }
}

pub async fn import_mcp_server(ctx: &Context, args: McpImport) -> Result<()> {
    let config_path = resolve_scope_profile(ctx, args.scope, args.profile.as_ref())?;
    todo!()
}

pub async fn get_mcp_server_status(ctx: &Context, name: String) -> Result<()> {
    todo!()
}

fn resolve_scope_profile(ctx: &Context, scope: Option<Scope>, profile: Option<&impl AsRef<str>>) -> Result<PathBuf> {
    Ok(match (scope, profile) {
        (None | Some(Scope::Workspace), _) => workspace_mcp_config_path(ctx)?,
        (Some(Scope::Global), _) => global_mcp_config_path(ctx)?,
        (Some(scope @ Scope::Profile), None) => bail!("profile must be specified for scope: {scope}"),
        (_, Some(profile)) => profile_mcp_config_path(ctx, profile)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scope_and_profile_defaults_to_workspace() {
        let ctx = Context::new();
        let path = resolve_scope_profile(&ctx, None, None::<&String>).unwrap();
        assert_eq!(
            path.to_str(),
            workspace_mcp_config_path(&ctx).unwrap().to_str(),
            "No scope or profile should default to the workspace path"
        );
    }
}
