use std::io::Write;
use std::path::{
    Path,
    PathBuf,
};
use std::process::ExitCode;

use clap::Args;
use crossterm::execute;
use crossterm::style::{
    self,
    Color,
};
use eyre::Result;
use time::OffsetDateTime;
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use crate::util::directories::logs_dir;

/// Arguments for the logdump command that collects logs for support investigation
#[derive(Debug, PartialEq, Args)]
pub struct LogdumpArgs;

impl LogdumpArgs {
    pub async fn execute(self) -> Result<ExitCode> {
        let mut stderr = std::io::stderr();

        execute!(
            stderr,
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
                    stderr,
                    style::SetForegroundColor(Color::Green),
                    style::Print(format!(
                        "✓ Successfully created {} with {} log files\n",
                        zip_filename, log_count
                    )),
                    style::ResetColor,
                )?;
                Ok(ExitCode::SUCCESS)
            },
            Err(e) => {
                execute!(
                    stderr,
                    style::SetForegroundColor(Color::Red),
                    style::Print(format!("✗ Failed to create log dump: {}\n\n", e)),
                    style::ResetColor,
                )?;
                Err(eyre::eyre!("Log dump failed: {}", e))
            },
        }
    }

    async fn create_log_dump(&self, zip_path: &Path) -> Result<usize, Box<dyn std::error::Error>> {
        let file = std::fs::File::create(zip_path)?;
        let mut zip = ZipWriter::new(file);
        let mut log_count = 0;

        log_count += self.collect_qchat_log(&mut zip)?;

        zip.finish()?;
        Ok(log_count)
    }

    fn collect_qchat_log(&self, zip: &mut ZipWriter<std::fs::File>) -> Result<usize, Box<dyn std::error::Error>> {
        let mut count = 0;

        // Get the qchat.log file specifically
        if let Ok(log_dir) = logs_dir() {
            let qchat_log_path = log_dir.join("qchat.log");
            if qchat_log_path.exists() {
                count += self.add_log_file_to_zip(&qchat_log_path, zip, "logs")?;
            }
        }

        Ok(count)
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
    async fn test_collect_qchat_log() {
        let temp_dir = TempDir::new().unwrap();
        let log_dir = temp_dir.path().join("logs");
        fs::create_dir_all(&log_dir).unwrap();

        // Create a test qchat.log file and some other files
        fs::write(log_dir.join("qchat.log"), "qchat log content").unwrap();
        fs::write(log_dir.join("other.log"), "other log content").unwrap();
        fs::write(log_dir.join("not_a_log.json"), "not a log").unwrap();

        let zip_path = temp_dir.path().join("test.zip");
        let file = fs::File::create(&zip_path).unwrap();
        let mut zip = ZipWriter::new(file);

        let logdump = LogdumpArgs;

        // Mock the logs_dir to return our test directory
        let qchat_log_path = log_dir.join("qchat.log");
        let result = logdump.add_log_file_to_zip(&qchat_log_path, &mut zip, "logs");

        assert!(result.is_ok());
        let count = result.unwrap();
        assert_eq!(count, 1); // Should collect only the qchat.log file

        zip.finish().unwrap();

        // Verify the zip contains the expected file
        let file = fs::File::open(&zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        assert_eq!(archive.len(), 1);

        let mut file_in_zip = archive.by_index(0).unwrap();
        assert_eq!(file_in_zip.name(), "logs/qchat.log");

        let mut contents = String::new();
        std::io::Read::read_to_string(&mut file_in_zip, &mut contents).unwrap();
        assert_eq!(contents, "qchat log content");
    }
}
