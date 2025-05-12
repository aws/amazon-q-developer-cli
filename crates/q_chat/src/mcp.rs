use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;

use eyre::{
    Result,
    bail,
};
use fig_os_shim::Context;
use fig_util::directories::chat_profiles_dir;
use tokio::fs;
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
use crate::tools::custom_tool::{
    CustomToolConfig,
    default_timeout,
};

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
    let scope = args.scope.unwrap_or(Scope::Workspace);
    let config_path = resolve_scope_profile(ctx, args.scope, args.profile.as_ref())?;

    if !ctx.fs().exists(&config_path) && scope != Scope::Profile {
        if let Some(parent) = config_path.parent() {
            ctx.fs().create_dir_all(parent).await?;
        }
        McpServerConfig::default().save_to_file(ctx, &config_path).await?;
        println!("📁 Created MCP config in'{}'", config_path.display());
    }

    let mut config: McpServerConfig = serde_json::from_str(&ctx.fs().read_to_string(&config_path).await?)?;

    if config.mcp_servers.contains_key(&args.name) && !args.force {
        bail!(
            "MCP server '{}' already exists in {} (scope {}). Use --force to overwrite.",
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

    config.mcp_servers.insert(args.name.clone(), tool);
    config.save_to_file(ctx, &config_path).await?;

    println!(
        "✓ Added MCP server '{}' to {scope}",
        args.name,
        scope = scope_display(&scope, &args.profile)
    );
    Ok(())
}

pub async fn remove_mcp_server(ctx: &Context, args: McpRemove) -> Result<()> {
    let scope = args.scope.unwrap_or(Scope::Workspace);
    let config_path = resolve_scope_profile(ctx, args.scope, args.profile.as_ref())?;

    if !ctx.fs().exists(&config_path) {
        println!("\n No MCP server configurations found.\n");
        return Ok(());
    }

    let mut config = McpServerConfig::load_from_file(ctx, &config_path).await?;
    match config.mcp_servers.remove(&args.name) {
        Some(_) => {
            config.save_to_file(ctx, &config_path).await?;
            println!(
                "✓ Removed MCP server '{}' from {}",
                args.name,
                scope_display(&scope, &args.profile)
            );
        },
        None => println!(
            "No MCP server named '{}' found in {}",
            args.name,
            scope_display(&scope, &args.profile)
        ),
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
        println!();
        println!("{}:", scope_display(&scope, &profile));
        println!("  {}", path.display());
        match cfg_opt {
            Some(cfg) if !cfg.mcp_servers.is_empty() => {
                for (name, tool_cfg) in &cfg.mcp_servers {
                    println!("    • {name:<12} {}", tool_cfg.command);
                }
            },
            _ => {
                println!("    (empty)");
            },
        }
    }
    Ok(())
}

pub async fn import_mcp_server(ctx: &Context, args: McpImport) -> Result<()> {
    let scope: Scope = args.scope.unwrap_or(Scope::Workspace);
    let config_path = resolve_scope_profile(ctx, args.scope, args.profile.as_ref())?;

    if !ctx.fs().exists(&config_path) && scope != Scope::Profile {
        if let Some(parent) = config_path.parent() {
            ctx.fs().create_dir_all(parent).await?;
        }
        McpServerConfig::default().save_to_file(ctx, &config_path).await?;
        println!("📁 Created MCP config in'{}'", config_path.display());
    }

    let src_path = expand_path(ctx, &args.file)?;
    let src_cfg: McpServerConfig = serde_json::from_str(&ctx.fs().read_to_string(&src_path).await?)?;
    let mut dst_cfg: McpServerConfig = McpServerConfig::load_from_file(ctx, &config_path).await?;

    let mut added = 0;
    for (name, cfg) in src_cfg.mcp_servers {
        let exists = dst_cfg.mcp_servers.contains_key(&name);
        if exists && !args.force {
            bail!(
                "MCP server '{}' already exists in {} (scope {}). Use --force to overwrite.",
                name,
                config_path.display(),
                scope
            );
        }
        dst_cfg.mcp_servers.insert(name.clone(), cfg);
        added +=1;
    }

    dst_cfg.save_to_file(ctx, &config_path).await?;

    println!(
        "✓ Imported {added} MCP server(s) into {:?} scope",
        scope_display(&scope, &args.profile)
    );
    Ok(())
}

pub async fn get_mcp_server_status(ctx: &Context, name: String) -> Result<()> {
    let configs = get_mcp_server_configs(ctx, None, None).await?;
    let mut found = false;

    for (sc, prof, path, cfg_opt) in configs {
        if let Some(cfg) = cfg_opt {
            if let Some(tool_cfg) = cfg.mcp_servers.get(&name) {
                found = true;
                println!("─────────────\n");
                match sc {
                    Scope::Workspace => println!("Scope   : workspace"),
                    Scope::Global => println!("Scope   : global"),
                    Scope::Profile => {
                        let p = prof.as_deref().unwrap_or("<unknown>");
                        println!("Scope   : profile ({p})");
                    },
                }
                println!("File    : {}", path.display());
                println!("Command : {}", tool_cfg.command);
                println!("Timeout : {} ms", tool_cfg.timeout);
                println!(
                    "Env Vars: {}",
                    tool_cfg
                        .env
                        .as_ref()
                        .map(|e| e.keys().cloned().collect::<Vec<_>>().join(", "))
                        .unwrap_or_else(|| "(none)".into())
                );
            }
        }
    }

    if !found {
        bail!("No MCP server named '{name}' found in any scope/profile");
    }

    Ok(())
}

async fn get_mcp_server_configs(
    ctx: &Context,
    scope: Option<Scope>,
    profile: Option<String>,
) -> Result<Vec<(Scope, Option<String>, PathBuf, Option<McpServerConfig>)>> {
    let mut all_profiles: Vec<String> = Vec::new();
    if let Ok(dir) = chat_profiles_dir(ctx) {
        if dir.exists() {
            let mut rd = fs::read_dir(&dir).await?;
            while let Some(entry) = rd.next_entry().await? {
                if entry.file_type().await?.is_dir() {
                    if let Some(name) = entry.file_name().to_str() {
                        all_profiles.push(name.to_owned());
                    }
                }
            }
        }
    }

    let targets: Vec<(Scope, Option<String>)> = match (scope, profile) {
        (Some(Scope::Workspace), _) => vec![(Scope::Workspace, None)],
        (Some(Scope::Global), _) => vec![(Scope::Global, None)],
        (Some(Scope::Profile), Some(p)) => vec![(Scope::Profile, Some(p))],
        (Some(Scope::Profile), None) => vec![(Scope::Profile, Some("default".to_string()))],
        (None, Some(p)) => vec![(Scope::Profile, Some(p))],

        // give nothing ⇒ default priority：workspace → global -> profile
        (None, None) => {
            let mut v = vec![(Scope::Workspace, None), (Scope::Global, None)];
            all_profiles.sort();
            for p in &all_profiles {
                v.push((Scope::Profile, Some(p.clone())));
            }
            v
        },
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
        Scope::Workspace => "📄 workspace".into(),
        Scope::Global => "🌍 global".into(),
        Scope::Profile => format!("👤 profile({})", profile.as_deref().unwrap_or("default")),
    }
}

fn resolve_scope_profile(ctx: &Context, scope: Option<Scope>, profile: Option<&impl AsRef<str>>) -> Result<PathBuf> {
    Ok(match (scope, profile) {
        (None | Some(Scope::Workspace), _) => workspace_mcp_config_path(ctx)?,
        (Some(Scope::Global), _) => global_mcp_config_path(ctx)?,
        (Some(scope @ Scope::Profile), None) => bail!("profile must be specified for scope: {scope}"),
        (_, Some(profile)) => profile_mcp_config_path(ctx, profile)?,
    })
}

fn expand_path(ctx: &Context, p: &str) -> Result<PathBuf> {
    let p = shellexpand::tilde(p);
    let mut path = PathBuf::from(p.as_ref());
    if path.is_relative() {
        path = ctx.env().current_dir()?.join(path);
    }
    Ok(path)
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
