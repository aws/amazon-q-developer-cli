use crate::database::settings::Setting;
use crate::os::Os;
use crate::cli::chat::conversation::ConversationState;
use chrono::Local;
use tracing::warn;

pub struct AutoSaveManager {
    session_filename: Option<String>,
}

impl AutoSaveManager {
    pub fn new() -> Self {
        Self {
            session_filename: None,
        }
    }

    pub async fn auto_save_if_enabled(
        &mut self,
        os: &Os,
        conversation: &ConversationState,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Check if auto-save is enabled
        let auto_save_enabled = os.database.settings.get_bool(Setting::ChatEnableAutoSave).unwrap_or(false);
        tracing::info!("Auto-save check: enabled={}", auto_save_enabled);
        
        if !auto_save_enabled {
            return Ok(());
        }

        // Generate filename on first save
        if self.session_filename.is_none() {
            let pattern = os.database.settings
                .get_string(Setting::ChatAutoSavePath)
                .unwrap_or_else(|| "auto-save-{timestamp}.json".to_string());
            
            let timestamp = Local::now().format("%Y%m%d-%H%M%S");
            let filename = pattern.replace("{timestamp}", &timestamp.to_string());
            tracing::info!("Auto-save: generating filename: {}", filename);
            self.session_filename = Some(filename);
        }

        // Execute auto-save
        if let Some(filename) = &self.session_filename {
            tracing::info!("Auto-save: attempting to save to {}", filename);
            match serde_json::to_string_pretty(conversation) {
                Ok(contents) => {
                    match os.fs.write(filename, contents).await {
                        Ok(_) => tracing::info!("Auto-save: successfully saved to {}", filename),
                        Err(e) => warn!("Auto-save failed: {}", e),
                    }
                }
                Err(e) => {
                    warn!("Auto-save serialization failed: {}", e);
                }
            }
        }

        Ok(())
    }
}
