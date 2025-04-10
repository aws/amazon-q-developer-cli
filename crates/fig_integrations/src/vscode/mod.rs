use std::env::temp_dir;
use std::io::BufRead;
use std::path::PathBuf;

use async_trait::async_trait;
use fig_util::macos::BUNDLE_CONTENTS_RESOURCE_PATH;
use macos_utils::url::path_for_application;
use tokio::process::Command;
use tracing::error;

use crate::error::Result;
use crate::{
    Error,
    Integration,
};

const OLD_EXTENSION_PREFIX: &str = "withfig.fig-";
const EXTENSION_PREFIX: &str = "amazonwebservices.codewhisperer-for-command-line-companion";

const EXTENSION_VERSION: &str = env!("CARGO_PKG_VERSION");
const EXTENSION: &[u8] = include_bytes!("vscode-plugin.vsix");

#[derive(Clone)]
pub struct VSCodeVariant {
    bundle_identifier: &'static str,
    config_folder_name: &'static str,
    application_support_folder_name: &'static str,
    pub application_name: &'static str,
    cli_executable_name: &'static str,
}

pub static VARIANTS: &[VSCodeVariant] = &[
    VSCodeVariant {
        bundle_identifier: "com.microsoft.VSCode",
        config_folder_name: ".vscode",
        application_support_folder_name: "Code",
        application_name: "VSCode",
        cli_executable_name: "code",
    },
    VSCodeVariant {
        bundle_identifier: "com.microsoft.VSCodeInsiders",
        config_folder_name: ".vscode-insiders",
        application_support_folder_name: "Code - Insiders",
        application_name: "VSCode Insiders",
        cli_executable_name: "code",
    },
    VSCodeVariant {
        bundle_identifier: "com.vscodium",
        config_folder_name: ".vscode-oss",
        application_support_folder_name: "VSCodium",
        application_name: "VSCodium",
        cli_executable_name: "codium",
    },
    VSCodeVariant {
        bundle_identifier: "com.visualstudio.code.oss",
        config_folder_name: ".vscode-oss",
        application_support_folder_name: "VSCodium",
        application_name: "VSCodium",
        cli_executable_name: "codium",
    },
    VSCodeVariant {
        bundle_identifier: "com.todesktop.230313mzl4w4u92",
        config_folder_name: ".cursor",
        application_support_folder_name: "Cursor",
        application_name: "Cursor",
        cli_executable_name: "cursor",
    },
    VSCodeVariant {
        bundle_identifier: "com.todesktop.230313mzl4w4u92",
        config_folder_name: ".cursor-nightly",
        application_support_folder_name: "Cursor Nightly",
        application_name: "Cursor Nightly",
        cli_executable_name: "cursor-nightly",
    },
    VSCodeVariant {
        bundle_identifier: "com.exafunction.windsurf",
        config_folder_name: ".windsurf",
        application_support_folder_name: "Windsurf",
        application_name: "Windsurf",
        cli_executable_name: "windsurf",
    },
    VSCodeVariant {
        bundle_identifier: "com.exafunction.windsurf-next",
        config_folder_name: ".windsurf-next",
        application_support_folder_name: "Windsurf Next",
        application_name: "Windsurf Next",
        cli_executable_name: "windsurf-next",
    },
    VSCodeVariant {
        bundle_identifier: "co.posit.positron",
        config_folder_name: ".positron",
        application_support_folder_name: "Positron",
        application_name: "Positron",
        cli_executable_name: "positron",
    },
    VSCodeVariant {
        bundle_identifier: "com.trae.app",
        config_folder_name: ".trae",
        application_support_folder_name: "Trae",
        application_name: "Trae",
        cli_executable_name: "trae",
    },
];

pub fn variants_installed() -> Vec<VSCodeVariant> {
    VARIANTS
        .iter()
        .filter(|variant| path_for_application(variant.bundle_identifier).is_some())
        .cloned()
        .collect()
}

/// Determines if the VSCode extension is installed by checking for the Amazon Q extension name in
/// the stdout of `code --list-extensions --show-versions`.
fn is_installed_from_stdout(stdout: impl AsRef<[u8]>) -> bool {
    let extension_name = format!("{EXTENSION_PREFIX}@{EXTENSION_VERSION}");
    stdout
        .as_ref()
        .lines()
        .any(|line| if let Ok(l) = line { l == extension_name } else { false })
}

pub struct VSCodeIntegration {
    pub variant: VSCodeVariant,
}

impl VSCodeIntegration {
    async fn update_settings(&self) -> Result<()> {
        let settings_path = fig_util::directories::home_dir()?
            .join("Library/Application Support")
            .join(self.variant.application_support_folder_name)
            .join("User/settings.json");

        let settings_content = if settings_path.exists() {
            tokio::fs::read_to_string(&settings_path).await?
        } else {
            if let Some(parent) = settings_path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            "{}".to_string()
        };

        let mut settings: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&settings_content)?;
        if settings.get("editor.accessibilitySupport").and_then(|x| x.as_str()) != Some("on") {
            settings.insert(
                "editor.accessibilitySupport".into(),
                serde_json::Value::String("off".into()),
            );

            let settings_new = serde_json::to_string_pretty(&settings)?;
            tokio::fs::write(settings_path, settings_new).await?;
        }

        Ok(())
    }

    fn extensions_dir(&self) -> Result<PathBuf> {
        Ok(fig_util::directories::home_dir()?
            .join(self.variant.config_folder_name)
            .join("extensions"))
    }

    fn cli_path(&self) -> Result<PathBuf> {
        let bundle_path = path_for_application(self.variant.bundle_identifier)
            .ok_or_else(|| Error::ApplicationNotInstalled(self.variant.application_name.into()))?;

        Ok(bundle_path
            .join(BUNDLE_CONTENTS_RESOURCE_PATH)
            .join("app/bin")
            .join(self.variant.cli_executable_name))
    }

    async fn remove_ext_by_prefix(&self, prefix: &str) -> Result<()> {
        let mut entries = tokio::fs::read_dir(self.extensions_dir()?).await?;
        while let Some(entry) = entries.next_entry().await? {
            if entry.file_name().to_string_lossy().starts_with(prefix) {
                tokio::fs::remove_dir_all(entry.path()).await?;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl Integration for VSCodeIntegration {
    fn describe(&self) -> String {
        format!("{} Integration", self.variant.application_name)
    }

    async fn install(&self) -> Result<()> {
        if self.is_installed().await.is_ok() {
            return Ok(());
        }

        self.remove_ext_by_prefix(OLD_EXTENSION_PREFIX).await.ok();

        if let Err(err) = self.update_settings().await {
            error!("error updating {} settings: {err:?}", self.variant.application_name);
        }

        let extension_path = temp_dir().join("codewhisperer-for-command-line-helper.vsix");
        tokio::fs::write(&extension_path, &EXTENSION).await?;

        let output = Command::new(self.cli_path()?)
            .arg("--install-extension")
            .arg(extension_path.as_os_str())
            .output()
            .await?;

        if !output.status.success() {
            return Err(Error::Custom(
                format!(
                    "error installing extension. stdout: {:?}",
                    String::from_utf8_lossy(&output.stdout)
                )
                .into(),
            ));
        }

        Ok(())
    }

    async fn uninstall(&self) -> Result<()> {
        self.remove_ext_by_prefix(EXTENSION_PREFIX).await
    }

    async fn is_installed(&self) -> Result<()> {
        let output = Command::new(self.cli_path()?)
            .arg("--list-extensions")
            .arg("--show-versions")
            .output()
            .await?;
        if !is_installed_from_stdout(output.stdout) {
            return Err(Error::Custom("Extension not installed".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extension_is_installed_from_stdout() {
        assert!(is_installed_from_stdout(indoc::formatdoc! {"
            [0423/155310.304438:ERROR:codesign_util.cc(108)] SecCodeCheckValidity: Error Domain=NSOSStatusErrorDomain Code=-67062 \"(null)\" (-67062)
            amazonwebservices.codewhisperer-for-command-line-companion@{EXTENSION_VERSION}
            eamodio.gitlens@14.9.0
            mathiasfrohlich.kotlin@1.7.1
        "}));

        assert!(!is_installed_from_stdout(indoc::indoc! {r#"
            [0423/155310.304438:ERROR:codesign_util.cc(108)] SecCodeCheckValidity: Error Domain=NSOSStatusErrorDomain Code=-67062 "(null)" (-67062)
            eamodio.gitlens@14.9.0
            mathiasfrohlich.kotlin@1.7.1
        "#}));
    }
}
