use std::collections::HashSet;
use std::path::PathBuf;

use async_trait::async_trait;
use fig_util::partitioned_compare;
use fig_util::terminal::IntelliJVariant;
use serde::Deserialize;
use tracing::debug;

use crate::error::Result;
use crate::{
    Error,
    Integration,
};

const PLUGIN_SLUG: &str = "jetbrains-extension";
const PLUGIN_JAR: &str = "jetbrains-extension-2.0.0.jar";
static PLUGIN_CONTENTS: &[u8] = include_bytes!("instrumented-codewhisperer-for-command-line-companion-1.0.4.jar");

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProductInfo {
    product_code: String,
    build_number: String,
}

pub async fn variants_installed() -> Result<Vec<IntelliJIntegration>> {
    let mut installed = HashSet::new();

    let toolbox_dir = dirs::data_dir()
        .ok_or_else(|| Error::Custom("couldn't get data dir".into()))?
        .join("JetBrains/Toolbox/apps");

    if !toolbox_dir.exists() {
        return Err(Error::Custom("toolbox apps dir missing".into()));
    }

    // find all the installed apps
    let mut toolbox_apps = tokio::fs::read_dir(toolbox_dir).await?;
    while let Some(app_dir) = toolbox_apps.next_entry().await? {
        if !app_dir.file_type().await?.is_dir() {
            continue;
        }
        debug!("searching {:?}", app_dir.file_name());
        let mut app_chs = tokio::fs::read_dir(app_dir.path()).await?;
        while let Some(app_ch) = app_chs.next_entry().await? {
            if !app_ch.file_type().await?.is_dir() {
                continue;
            }
            let mut versioned_dirs = tokio::fs::read_dir(app_ch.path()).await?;
            let mut versions = Vec::new();
            let mut variant = None;
            while let Some(versioned_dir) = versioned_dirs.next_entry().await? {
                if !versioned_dir.file_type().await?.is_dir() {
                    continue;
                }
                // now we need to find where ever the product info file is
                let product_info_path = versioned_dir.path().join("product-info.json");
                if product_info_path.exists() {
                    let product_info_raw = tokio::fs::read(product_info_path).await?;
                    let product_info: ProductInfo = serde_json::from_slice(&product_info_raw)?;
                    if let Some(product_variant) = IntelliJVariant::from_product_code(&product_info.product_code) {
                        variant = Some(product_variant);
                        versions.push((product_info.build_number, versioned_dir.path()));
                    }
                }
            }
            if let Some(variant) = variant {
                versions.sort_by(|lhs, rhs| partitioned_compare(&lhs.0, &rhs.0, '.'));
                if let Some((_, data_path)) = versions.last() {
                    installed.insert(IntelliJIntegration {
                        variant,
                        data_path: data_path.clone(),
                    });
                }
            }
        }
    }

    Ok(installed.into_iter().collect())
}

#[derive(Hash, PartialEq, Eq)]
pub struct IntelliJIntegration {
    pub variant: IntelliJVariant,
    data_path: PathBuf,
}

#[async_trait]
impl Integration for IntelliJIntegration {
    fn describe(&self) -> String {
        format!("{} Integration", self.variant.application_name())
    }

    async fn install(&self) -> Result<()> {
        let plugins_folder = self.data_path.join("plugins");
        if !plugins_folder.exists() {
            tokio::fs::create_dir(&plugins_folder).await?;
        }

        let plugin_folder = plugins_folder.join(PLUGIN_SLUG);

        if plugin_folder.exists() {
            tokio::fs::remove_dir_all(&plugin_folder).await.map_err(|err| {
                Error::Custom(format!("Failed removing destination folder {plugin_folder:?}: {err:?}").into())
            })?;
        }

        let lib_dir = plugin_folder.join("lib");
        tokio::fs::create_dir_all(&lib_dir).await?;

        let jar_path = lib_dir.join(PLUGIN_JAR);

        tokio::fs::write(&jar_path, PLUGIN_CONTENTS)
            .await
            .map_err(|err| Error::Custom(format!("Failed writing plugin jar to {jar_path:?}: {err:?}").into()))?;

        Ok(())
    }

    async fn uninstall(&self) -> Result<()> {
        let plugin_folder = self.data_path.join("plugins").join(PLUGIN_SLUG);

        if plugin_folder.exists() {
            tokio::fs::remove_dir_all(&plugin_folder).await.map_err(|err| {
                Error::Custom(format!("Failed removing destination folder {plugin_folder:?}: {err:?}").into())
            })?;
        }

        Ok(())
    }

    async fn is_installed(&self) -> Result<()> {
        let plugin_folder = self.data_path.join("plugins").join(PLUGIN_SLUG);

        if !plugin_folder.exists() {
            return Err(Error::Custom("Plugin not installed".into()));
        }

        Ok(())
    }
}
