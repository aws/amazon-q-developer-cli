use std::io::Write;
use std::path::{
    Path,
    PathBuf,
};

use clap::Args;
use crossterm::execute;
use crossterm::style::{
    self,
    Color,
};
use time::OffsetDateTime;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::util::directories::logs_dir;

/// Arguments for the logdump command that collects logs for support investigation
#[derive(Debug, PartialEq, Args)]
pub struct LogdumpArgs;

impl LogdumpArgs {
    pub async fn execute(self, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        execute!(
            session.stderr,
            style::SetForegroundColor(Color::Cyan),
            style::Print("Collecting logs...\n"),
            style::ResetColor,
        )?;

        let timestamp = OffsetDateTime::now_local()
            .unwrap_or_else(|_| OffsetDateTime::now_utc())
            .format(&time::format_description::well_known::Iso8601::DEFAULT)
            .unwrap_or_else(|_| "unknown".to_string())
            .replace(':', "-"); // Replace colons for Windows compatibility

        let zip_filename = format!("q-logs-{}.zip", timestamp);
        let zip_path = PathBuf::from(&zip_filename);

        match self.create_log_dump(&zip_path).await {
            Ok(log_count) => {
                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Green),
                    style::Print(format!(
                        "✓ Successfully created {} with {} log files\n",
                        zip_filename, log_count
                    )),
                    style::ResetColor,
                )?;
            },
            Err(e) => {
                execute!(
                    session.stderr,
                    style::SetForegroundColor(Color::Red),
                    style::Print(format!("✗ Failed to create log dump: {}\n\n", e)),
                    style::ResetColor,
                )?;
                return Err(ChatError::Custom(format!("Log dump failed: {}", e).into()));
            },
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    async fn create_log_dump(&self, zip_path: &Path) -> Result<usize, Box<dyn std::error::Error>> {
        let file = std::fs::File::create(zip_path)?;
        let mut zip = ZipWriter::new(file);
        let mut log_count = 0;

        // Only collect qchat.log (keeping current implementation logic)
        log_count += self.collect_qchat_log(&mut zip)?;

        zip.finish()?;
        Ok(log_count)
    }

    fn collect_qchat_log(&self, zip: &mut ZipWriter<std::fs::File>) -> Result<usize, Box<dyn std::error::Error>> {
        // Use the unified logs_dir function to get the correct log directory
        if let Ok(log_dir) = logs_dir() {
            let qchat_log_path = log_dir.join("qchat.log");
            if qchat_log_path.exists() {
                return self.add_log_file_to_zip(&qchat_log_path, zip, "logs");
            }
        }
        Ok(0)
    }

    fn add_log_file_to_zip(
        &self,
        path: &Path,
        zip: &mut ZipWriter<std::fs::File>,
        prefix: &str,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let content = std::fs::read(path)?;
        let filename = format!(
            "{}/{}",
            prefix,
            path.file_name()
                .unwrap_or_else(|| std::ffi::OsStr::new("unknown.log"))
                .to_string_lossy()
        );

        zip.start_file(filename, SimpleFileOptions::default())?;
        zip.write_all(&content)?;
        Ok(1)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[tokio::test]
    async fn test_logdump_creates_zip_file() {
        let temp_dir = TempDir::new().unwrap();
        let zip_path = temp_dir.path().join("test-logs.zip");

        let logdump = LogdumpArgs;

        // Create the zip file (even if no logs are found, it should create an empty zip)
        let result = logdump.create_log_dump(&zip_path).await;

        // The function should succeed and create a zip file
        assert!(result.is_ok());
        assert!(zip_path.exists());

        // Verify it's a valid zip file by trying to read it
        let file = fs::File::open(&zip_path).unwrap();
        let archive = zip::ZipArchive::new(file);
        assert!(archive.is_ok());
    }

    #[tokio::test]
    async fn test_add_log_file_to_zip() {
        let temp_dir = TempDir::new().unwrap();
        let log_dir = temp_dir.path().join("logs");
        fs::create_dir_all(&log_dir).unwrap();

        // Create a test qchat.log file
        let qchat_log_path = log_dir.join("qchat.log");
        fs::write(&qchat_log_path, "test log content").unwrap();

        let zip_path = temp_dir.path().join("test.zip");
        let file = fs::File::create(&zip_path).unwrap();
        let mut zip = ZipWriter::new(file);

        let logdump = LogdumpArgs;

        // Mock the logs_dir to return our test directory
        let result = logdump.add_log_file_to_zip(&qchat_log_path, &mut zip, "logs");

        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);
    }
}
