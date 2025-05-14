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
use tokio::fs;
use tracing::warn;

use crate::cli::chat::cli::{
    Mcp,
    McpAdd,
    McpImport,
    McpList,
    McpRemove,
    Scope,
};
use crate::cli::chat::tool_manager::{
    McpServerConfig,
    global_mcp_config_path,
    profile_mcp_config_path,
    workspace_mcp_config_path,
};
use crate::cli::chat::tools::custom_tool::{
    CustomToolConfig,
    default_timeout,
};
use crate::cli::chat::util::shared_writer::SharedWriter;
use crate::platform::Context;
use crate::util::directories::chat_profiles_dir;

pub async fn execute_mcp(args: Mcp) -> Result<ExitCode> {
    let ctx = Context::new();
    let mut output = SharedWriter::stdout();

    match args {
        Mcp::Add(args) => add_mcp_server(&ctx, &mut output, args).await?,
        Mcp::Remove(args) => remove_mcp_server(&ctx, &mut output, args).await?,
        Mcp::List(args) => list_mcp_server(&ctx, &mut output, args).await?,
        Mcp::Import(args) => import_mcp_server(&ctx, &mut output, args).await?,
        Mcp::Status { name } => get_mcp_server_status(&ctx, &mut output, name).await?,
    }

    output.flush()?;
    Ok(ExitCode::SUCCESS)
}

pub async fn add_mcp_server(ctx: &Context, output: &mut SharedWriter, args: McpAdd) -> Result<()> {
    let scope = args.scope.unwrap_or(Scope::Workspace);
    let config_path = resolve_scope_profile(ctx, args.scope, args.profile.as_ref())?;

    let mut config: McpServerConfig = ensure_config_file(ctx, &config_path, scope, output).await?;

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
    writeln!(
        output,
        "✓ Added MCP server '{}' to {}",
        args.name,
        scope_display(&scope, &args.profile)
    )?;
    Ok(())
}

pub async fn remove_mcp_server(ctx: &Context, output: &mut SharedWriter, args: McpRemove) -> Result<()> {
    let scope = args.scope.unwrap_or(Scope::Workspace);
    let config_path = resolve_scope_profile(ctx, args.scope, args.profile.as_ref())?;

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
                "✓ Removed MCP server '{}' from {}",
                args.name,
                scope_display(&scope, &args.profile)
            )?;
        },
        None => {
            writeln!(
                output,
                "No MCP server named '{}' found in {}",
                args.name,
                scope_display(&scope, &args.profile)
            )?;
        },
    }
    Ok(())
}

pub async fn list_mcp_server(ctx: &Context, output: &mut SharedWriter, args: McpList) -> Result<()> {
    let configs = get_mcp_server_configs(ctx, args.scope, args.profile).await?;
    if configs.is_empty() {
        writeln!(output, "No MCP server configurations found.\n")?;
        return Ok(());
    }

    for (scope, profile, path, cfg_opt) in configs {
        writeln!(output)?;
        writeln!(output, "{}:\n  {}", scope_display(&scope, &profile), path.display())?;
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
    let config_path = resolve_scope_profile(ctx, args.scope, args.profile.as_ref())?;
    let mut dst_cfg = ensure_config_file(ctx, &config_path, scope, output).await?;

    let src_path = expand_path(ctx, &args.file)?;
    let src_cfg: McpServerConfig = McpServerConfig::load_from_file(ctx, &src_path).await?;

    let mut added = 0;
    for (name, cfg) in src_cfg.mcp_servers {
        if dst_cfg.mcp_servers.contains_key(&name) && !args.force {
            bail!(
                "MCP server '{}' already exists in {} (scope {}). Use --force to overwrite.",
                name,
                config_path.display(),
                scope
            );
        }
        dst_cfg.mcp_servers.insert(name.clone(), cfg);
        added += 1;
    }

    dst_cfg.save_to_file(ctx, &config_path).await?;
    writeln!(
        output,
        "✓ Imported {added} MCP server(s) into {}",
        scope_display(&scope, &args.profile)
    )?;
    Ok(())
}

pub async fn get_mcp_server_status(ctx: &Context, output: &mut SharedWriter, name: String) -> Result<()> {
    let configs = get_mcp_server_configs(ctx, None, None).await?;
    let mut found = false;

    for (sc, prof, path, cfg_opt) in configs {
        if let Some(cfg) = cfg_opt.and_then(|c| c.mcp_servers.get(&name).cloned()) {
            found = true;
            execute!(
                output,
                style::Print("\n─────────────\n"),
                style::Print(format!("Scope   : {}\n", scope_display(&sc, &prof))),
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
        bail!("No MCP server named '{name}' found in any scope/profile");
    }
    Ok(())
}

async fn get_mcp_server_configs(
    ctx: &Context,
    scope: Option<Scope>,
    profile: Option<String>,
) -> Result<Vec<(Scope, Option<String>, PathBuf, Option<McpServerConfig>)>> {
    let all_profiles = if matches!((scope, &profile), (None, None)) {
        let mut v = Vec::new();
        if let Ok(dir) = chat_profiles_dir(ctx) {
            let mut rd = fs::read_dir(dir).await?;
            while let Some(e) = rd.next_entry().await? {
                if e.file_type().await?.is_dir() {
                    if let Some(n) = e.file_name().to_str() {
                        v.push(n.to_owned());
                    }
                }
            }
        }
        v
    } else {
        Vec::new()
    };

    let mut targets = Vec::new();
    match (scope, profile) {
        (Some(Scope::Workspace), _) => targets.push((Scope::Workspace, None)),
        (Some(Scope::Global), _) => targets.push((Scope::Global, None)),
        (Some(Scope::Profile), Some(p)) => targets.push((Scope::Profile, Some(p))),
        (None, Some(p)) => targets.push((Scope::Profile, Some(p))),
        (None, None) => {
            targets.extend([(Scope::Workspace, None), (Scope::Global, None)]);
            for p in all_profiles {
                targets.push((Scope::Profile, Some(p)));
            }
        },
        (Some(Scope::Profile), None) => bail!("profile must be specified for scope: profile"),
    }

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

async fn ensure_config_file(
    ctx: &Context,
    path: &PathBuf,
    scope: Scope,
    out: &mut SharedWriter,
) -> Result<McpServerConfig> {
    if !ctx.fs().exists(path) && scope != Scope::Profile {
        if let Some(parent) = path.parent() {
            ctx.fs().create_dir_all(parent).await?;
        }
        McpServerConfig::default().save_to_file(ctx, path).await?;
        writeln!(out, "📁 Created MCP config in '{}'", path.display())?;
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

    #[test]
    fn test_resolve_profile_without_name_err() {
        let ctx = Context::new();
        assert!(resolve_scope_profile(&ctx, Some(Scope::Profile), None::<&String>).is_err());
    }

    #[test]
    fn test_resolve_paths() {
        let ctx = Context::new();
        // workspace
        let p = resolve_scope_profile(&ctx, Some(Scope::Workspace), None::<&String>).unwrap();
        assert_eq!(p, workspace_mcp_config_path(&ctx).unwrap());

        // global
        let p = resolve_scope_profile(&ctx, Some(Scope::Global), None::<&String>).unwrap();
        assert_eq!(p, global_mcp_config_path(&ctx).unwrap());

        // profile
        let p = resolve_scope_profile(&ctx, Some(Scope::Profile), Some(&"qa")).unwrap();
        assert_eq!(p, profile_mcp_config_path(&ctx, "qa").unwrap(), "profile path mismatch");
    }

    #[ignore = "TODO: fix in CI"]
    #[tokio::test]
    async fn ensure_file_created_and_loaded() {
        let ctx = Context::new();
        let mut out = SharedWriter::null();
        let path = workspace_mcp_config_path(&ctx).unwrap();

        let cfg = super::ensure_config_file(&ctx, &path, Scope::Workspace, &mut out)
            .await
            .unwrap();
        assert!(path.exists(), "config file should be created");
        assert!(cfg.mcp_servers.is_empty());
    }

    #[tokio::test]
    async fn add_then_remove_cycle() {
        use crate::cli::chat::cli::{
            McpAdd,
            McpRemove,
        };

        let ctx = Context::new();
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
}
