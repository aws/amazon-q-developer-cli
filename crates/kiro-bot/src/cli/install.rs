//! Install and uninstall bot instances.

use std::io::{
    self,
    Write,
};
use std::path::Path;

use anyhow::{
    Context,
    Result,
    bail,
};
use kiro_bot::config::{
    self,
    FrontendConfig,
};

pub fn cmd_install(source_path: &str) -> Result<()> {
    let source = Path::new(source_path).canonicalize().context("source path not found")?;
    let cfg = config::load_config(&source)?;
    let name = &cfg.name;

    let instance_dir = config::config_dir(name)?;
    if instance_dir.exists() {
        bail!("Instance '{name}' already installed at {}", instance_dir.display());
    }

    copy_dir(&source, &instance_dir)?;
    println!("Copied config to {}", instance_dir.display());

    let secrets_toml = match &cfg.frontend {
        FrontendConfig::Slack { .. } => prompt_slack_secrets()?,
        FrontendConfig::Cron { .. } => String::new(),
    };

    if !secrets_toml.is_empty() {
        let secrets_path = instance_dir.join("secrets.toml");
        std::fs::write(&secrets_path, secrets_toml)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&secrets_path, std::fs::Permissions::from_mode(0o600))?;
        }
        println!("Secrets written to {}", secrets_path.display());
    }

    println!("✅ Installed '{name}'. Run: kiro-bot start {name}");
    Ok(())
}

pub fn cmd_uninstall(name: &str) -> Result<()> {
    let instance_dir = config::config_dir(name)?;
    if !instance_dir.exists() {
        bail!("Instance '{name}' not found");
    }

    print!("Uninstall '{name}' and delete {}? [y/N] ", instance_dir.display());
    io::stdout().flush()?;
    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    if input.trim().to_lowercase() != "y" {
        println!("Aborted.");
        return Ok(());
    }

    let _ = crate::cli::service::cmd_stop(name);
    std::fs::remove_dir_all(&instance_dir)?;
    println!("✅ Uninstalled '{name}'");
    Ok(())
}

fn prompt_slack_secrets() -> Result<String> {
    let bot_token = rpassword::prompt_password("Slack bot token (xoxb-...): ").context("failed to read secret")?;
    let app_token = rpassword::prompt_password("Slack app token (xapp-...): ").context("failed to read secret")?;
    Ok(format!(
        "[slack]\nbot_token = \"{bot_token}\"\napp_token = \"{app_token}\"\n"
    ))
}

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let dst_path = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &dst_path)?;
        } else {
            std::fs::copy(entry.path(), dst_path)?;
        }
    }
    Ok(())
}
