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

        log_count += self.collect_chat_logs(&mut zip)?;

        zip.finish()?;
        Ok(log_count)
    }

    fn collect_chat_logs(&self, zip: &mut ZipWriter<std::fs::File>) -> Result<usize, Box<dyn std::error::Error>> {
        let mut count = 0;

        // Use the unified logs_dir function to get the correct log directory
        // This will recursively collect all log files from $TMPDIR/qlog
        if let Ok(log_dir) = logs_dir() {
            if log_dir.exists() {
                count += self.collect_logs_from_dir(&log_dir, zip, "logs")?;
            }
        }

        Ok(count)
    }

    fn collect_logs_from_dir(
        &self,
        dir: &Path,
        zip: &mut ZipWriter<std::fs::File>,
        prefix: &str,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let mut count = 0;
        let entries = std::fs::read_dir(dir)?;

        for entry in entries.flatten() {
            let path = entry.path();

            if path.is_file() && self.is_log_file(&path) {
                count += self.add_log_file_to_zip(&path, zip, prefix)?;
            } else if path.is_dir() {
                count += self.collect_logs_from_subdir(&path, zip, prefix)?;
            }
        }

        Ok(count)
    }

    fn is_log_file(&self, path: &Path) -> bool {
        path.extension().map(|ext| ext == "log").unwrap_or(false)
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

    fn collect_logs_from_subdir(
        &self,
        path: &Path,
        zip: &mut ZipWriter<std::fs::File>,
        prefix: &str,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let subdir_name = path
            .file_name()
            .unwrap_or_else(|| std::ffi::OsStr::new("unknown"))
            .to_string_lossy();
        let subdir_prefix = format!("{}/{}", prefix, subdir_name);
        self.collect_logs_from_dir(path, zip, &subdir_prefix)
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
    async fn test_collect_logs_from_dir() {
        let temp_dir = TempDir::new().unwrap();
        let log_dir = temp_dir.path().join("logs");
        fs::create_dir_all(&log_dir).unwrap();

        // Create some test log files
        fs::write(log_dir.join("test1.log"), "log content 1").unwrap();
        fs::write(log_dir.join("test2.log"), "log content 2").unwrap();
        fs::write(log_dir.join("not_a_log.json"), "not a log").unwrap();

        let zip_path = temp_dir.path().join("test.zip");
        let file = fs::File::create(&zip_path).unwrap();
        let mut zip = ZipWriter::new(file);

        let logdump = LogdumpArgs;
        let result = logdump.collect_logs_from_dir(&log_dir, &mut zip, "test_logs");

        assert!(result.is_ok());
        let count = result.unwrap();
        assert_eq!(count, 2); // Should collect .log files, but not .json

        zip.finish().unwrap();

        // Verify the zip contains the expected files
        let file = fs::File::open(&zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        assert_eq!(archive.len(), 2);

        let names: Vec<String> = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect();

        assert!(names.contains(&"test_logs/test1.log".to_string()));
        assert!(names.contains(&"test_logs/test2.log".to_string()));
    }
}
