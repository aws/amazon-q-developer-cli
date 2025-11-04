use std::io::Write;
use std::path::{
    Path,
    PathBuf,
};

use chrono::Utc;
use clap::Args;
use crossterm::execute;
use crossterm::style::{
    self,
};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

use crate::cli::chat::{
    ChatError,
    ChatSession,
    ChatState,
};
use crate::theme::StyledText;
use crate::util::paths::logs_dir;

/// Arguments for the logdump command that collects logs for support investigation
#[derive(Debug, PartialEq, Args)]
pub struct LogdumpArgs {
    /// Include MCP logs
    #[arg(long)]
    pub mcp: bool,
}

impl LogdumpArgs {
    pub async fn execute(self, session: &mut ChatSession) -> Result<ChatState, ChatError> {
        execute!(
            session.stderr,
            StyledText::brand_fg(),
            style::Print("Collecting logs...\n"),
            StyledText::reset(),
        )?;

        let timestamp = Utc::now().format("%Y-%m-%dT%H-%M-%SZ").to_string();
        let zip_filename = format!("q-logs-{timestamp}.zip");
        let zip_path: PathBuf = PathBuf::from(&zip_filename);
        let logs_directory =
            logs_dir().map_err(|e| ChatError::Custom(format!("Failed to get logs directory: {e}").into()))?;

        match self.create_log_dump(&zip_path, logs_directory).await {
            Ok(log_count) => {
                execute!(
                    session.stderr,
                    StyledText::success_fg(),
                    style::Print(format!(
                        "✓ Successfully created {zip_filename} with {log_count} log files\n"
                    )),
                    StyledText::reset(),
                )?;
            },
            Err(e) => {
                execute!(
                    session.stderr,
                    StyledText::error_fg(),
                    style::Print(format!("✗ Failed to create log dump: {e}\n\n")),
                    StyledText::reset(),
                )?;
                return Err(ChatError::Custom(format!("Log dump failed: {e}").into()));
            },
        }

        Ok(ChatState::PromptUser {
            skip_printing_tools: true,
        })
    }

    async fn create_log_dump(&self, zip_path: &Path, logs_dir: PathBuf) -> Result<usize, Box<dyn std::error::Error>> {
        let file = std::fs::File::create(zip_path)?;
        let mut zip = ZipWriter::new(file);
        let mut log_count = 0;

        // Collect qchat.log
        log_count += Self::collect_qchat_log(&mut zip, &logs_dir)?;

        // Collect mcp.log if --mcp flag is set
        if self.mcp {
            log_count += Self::collect_mcp_log(&mut zip, &logs_dir)?;
        }

        zip.finish()?;
        Ok(log_count)
    }

    fn collect_qchat_log(
        zip: &mut ZipWriter<std::fs::File>,
        logs_dir: &Path,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let qchat_log_path = logs_dir.join("qchat.log");
        if qchat_log_path.exists() {
            return Self::add_log_file_to_zip(&qchat_log_path, zip, "logs");
        }
        Ok(0)
    }

    fn collect_mcp_log(
        zip: &mut ZipWriter<std::fs::File>,
        logs_dir: &Path,
    ) -> Result<usize, Box<dyn std::error::Error>> {
        let mcp_log_path = logs_dir.join("mcp.log");
        if mcp_log_path.exists() {
            return Self::add_log_file_to_zip(&mcp_log_path, zip, "logs");
        }
        Ok(0)
    }

    fn add_log_file_to_zip(
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
    async fn test_logdump_creates_empty_zip_when_no_logs() {
        let temp_dir = TempDir::new().unwrap();
        let zip_path = temp_dir.path().join("test-logs.zip");
        let logs_dir = temp_dir.path().join("logs");
        fs::create_dir_all(&logs_dir).unwrap();

        let logdump = LogdumpArgs { mcp: false };

        // Create the zip file (even if no logs are found, it should create an empty zip)
        let result = logdump.create_log_dump(&zip_path, logs_dir).await;

        // The function should succeed and create a zip file with 0 log files
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
        assert!(zip_path.exists());

        // Verify it's a valid zip file by trying to read it
        let file = fs::File::open(&zip_path).unwrap();
        let archive = zip::ZipArchive::new(file);
        assert!(archive.is_ok());
        assert_eq!(archive.unwrap().len(), 0);
    }

    #[tokio::test]
    async fn test_logdump_includes_qchat_log() {
        let temp_dir = TempDir::new().unwrap();
        let zip_path = temp_dir.path().join("test-logs.zip");
        let logs_dir = temp_dir.path().join("logs");
        fs::create_dir_all(&logs_dir).unwrap();

        // Create a test qchat.log file
        let qchat_log_path = logs_dir.join("qchat.log");
        fs::write(&qchat_log_path, "test log content").unwrap();

        let logdump = LogdumpArgs { mcp: false };

        let result = logdump.create_log_dump(&zip_path, logs_dir).await;

        // The function should succeed and include 1 log file
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 1);
        assert!(zip_path.exists());

        // Verify the zip contains the log file
        let file = fs::File::open(&zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        assert_eq!(archive.len(), 1);

        let mut log_file = archive.by_name("logs/qchat.log").unwrap();
        let mut contents = String::new();
        std::io::Read::read_to_string(&mut log_file, &mut contents).unwrap();
        assert_eq!(contents, "test log content");
    }

    #[tokio::test]
    async fn test_logdump_includes_qchat_log_with_mcp_log() {
        let temp_dir = TempDir::new().unwrap();
        let zip_path = temp_dir.path().join("test-logs.zip");
        let logs_dir = temp_dir.path().join("logs");
        fs::create_dir_all(&logs_dir).unwrap();

        // Create test log files
        let qchat_log_path = logs_dir.join("qchat.log");
        fs::write(&qchat_log_path, "qchat log content").unwrap();
        let mcp_log_path = logs_dir.join("mcp.log");
        fs::write(&mcp_log_path, "mcp log content").unwrap();

        let logdump = LogdumpArgs { mcp: true };

        let result = logdump.create_log_dump(&zip_path, logs_dir).await;

        // The function should succeed and include 2 log files
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 2);
        assert!(zip_path.exists());

        // Verify the zip contains both log files
        let file = fs::File::open(&zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        assert_eq!(archive.len(), 2);

        {
            let mut qchat_file = archive.by_name("logs/qchat.log").unwrap();
            let mut qchat_contents = String::new();
            std::io::Read::read_to_string(&mut qchat_file, &mut qchat_contents).unwrap();
            assert_eq!(qchat_contents, "qchat log content");
        }

        {
            let mut mcp_file = archive.by_name("logs/mcp.log").unwrap();
            let mut mcp_contents = String::new();
            std::io::Read::read_to_string(&mut mcp_file, &mut mcp_contents).unwrap();
            assert_eq!(mcp_contents, "mcp log content");
        }
    }
}
