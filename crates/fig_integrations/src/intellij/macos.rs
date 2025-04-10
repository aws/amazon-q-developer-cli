use std::collections::HashMap;
use std::path::PathBuf;

use async_trait::async_trait;
use fig_util::macos::BUNDLE_CONTENTS_INFO_PLIST_PATH;
use fig_util::terminal::IntelliJVariant;
use macos_utils::url::path_for_application;
use serde::Deserialize;

use crate::error::Result;
use crate::{
    Error,
    Integration,
};

const PLUGIN_PREFIX: &str = "q-for-command-line-companion";

/// Version of the Amazon Q Intellij plugin. This should match the version in
/// [build.gradle](../../../../extensions/jetbrains/build.gradle).
const PLUGIN_VERSION: &str = "1.0.4";

/// Legacy plugin names that should be removed on uninstall.
const OLD_PLUGIN_SLUGS: [&str; 2] = ["codewhisperer-for-command-line-companion", "jetbrains-extension-2.0.0"];

static PLUGIN_CONTENTS: &[u8] = include_bytes!("instrumented-codewhisperer-for-command-line-companion-1.0.4.jar");

/// Returns the identifier of the plugin - i.e., the directory name of the Amazon Q plugin stored
/// under the Intellij `plugins/` directory.
fn plugin_slug() -> String {
    format!("{PLUGIN_PREFIX}-{PLUGIN_VERSION}")
}

pub async fn variants_installed() -> Result<Vec<IntelliJIntegration>> {
    Ok(IntelliJVariant::all()
        .iter()
        .filter(|variant| path_for_application(variant.bundle_identifier()).is_some())
        .cloned()
        .map(|variant| IntelliJIntegration { variant })
        .collect())
}

pub struct IntelliJIntegration {
    pub variant: IntelliJVariant,
}

#[derive(Deserialize)]
struct InfoPList {
    #[serde(rename = "JVMOptions")]
    jvm_options: JVMOptions,
}

#[derive(Deserialize)]
struct JVMOptions {
    #[serde(rename = "Properties")]
    properties: HashMap<String, String>,
}

impl IntelliJIntegration {
    fn get_jvm_properties(&self) -> Result<HashMap<String, String>> {
        let plist_path = path_for_application(self.variant.bundle_identifier())
            .ok_or_else(|| Error::ApplicationNotInstalled(self.variant.application_name().into()))?
            .join(BUNDLE_CONTENTS_INFO_PLIST_PATH);

        let contents: InfoPList = plist::from_file(plist_path)
            .map_err(|err| Error::Custom(format!("Could not read plist file: {err:?}").into()))?;

        Ok(contents.jvm_options.properties)
    }

    pub fn application_folder(&self) -> Result<PathBuf> {
        let mut props = self.get_jvm_properties().map_err(|err| {
            Error::Custom(
                format!(
                    "Couldn't get JVM properties for {}: {err:?}",
                    self.variant.application_name()
                )
                .into(),
            )
        })?;

        let selector = props.remove("idea.paths.selector").ok_or_else(|| {
            Error::Custom(
                format!(
                    "Could not read `idea.paths.selector` from jvm options for {}",
                    self.variant.application_name()
                )
                .into(),
            )
        })?;

        Ok(dirs::data_local_dir()
            .ok_or_else(|| Error::Custom("Could not read application support directory".into()))?
            .join(self.variant.organization())
            .join(selector))
    }
}

#[async_trait]
impl Integration for IntelliJIntegration {
    fn describe(&self) -> String {
        format!("{} Integration", self.variant.application_name())
    }

    async fn install(&self) -> Result<()> {
        if self.is_installed().await.is_ok() {
            return Ok(());
        }

        let application_folder = self.application_folder()?;
        if !application_folder.exists() {
            return Err(Error::Custom(
                format!(
                    "Application folder does not exist for {}: {application_folder:?}",
                    self.variant.application_name()
                )
                .into(),
            ));
        }

        let _ = self.uninstall().await;

        let plugins_folder = application_folder.join("plugins");
        tokio::fs::create_dir_all(&plugins_folder).await?;

        let plugin_slug = plugin_slug();
        let lib_dir = plugins_folder.join(&plugin_slug).join("lib");
        tokio::fs::create_dir_all(&lib_dir)
            .await
            .map_err(|err| Error::Custom(format!("Failed creating plugin lib folder: {err:?}").into()))?;

        let jar_path = lib_dir.join(format!("{}.jar", &plugin_slug));
        tokio::fs::write(&jar_path, PLUGIN_CONTENTS)
            .await
            .map_err(|err| Error::Custom(format!("Failed writing plugin jar to {jar_path:?}: {err:?}").into()))?;

        Ok(())
    }

    async fn uninstall(&self) -> Result<()> {
        let plugins_folder = self.application_folder()?.join("plugins");

        let mut entries = tokio::fs::read_dir(&plugins_folder).await.map_err(|err| {
            Error::Custom(format!("Failed reading plugins folder dir {plugins_folder:?}: {err:?}").into())
        })?;
        while let Some(entry) = entries.next_entry().await.map_err(|err| {
            Error::Custom(format!("Failed reading next entry in plugins folder dir {plugins_folder:?}: {err:?}").into())
        })? {
            let fname = entry.file_name().to_string_lossy().into_owned();
            if fname.starts_with(PLUGIN_PREFIX) || OLD_PLUGIN_SLUGS.iter().any(|slug| fname.starts_with(slug)) {
                tokio::fs::remove_dir_all(entry.path()).await.map_err(|err| {
                    Error::Custom(
                        format!(
                            "Failed removing entry {:?} from plugins folder dir {plugins_folder:?}: {err:?}",
                            entry.path()
                        )
                        .into(),
                    )
                })?;
            }
        }

        Ok(())
    }

    async fn is_installed(&self) -> Result<()> {
        let plugin_folder = self.application_folder()?.join("plugins").join(plugin_slug());
        if !plugin_folder.exists() {
            return Err(Error::Custom("Plugin not installed".into()));
        }

        Ok(())
    }
}
