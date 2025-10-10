use regex::Regex;
use std::path::Path;

/// File injection functionality for @filepath patterns
pub struct FileInjector;

impl FileInjector {
    /// Processes input text and injects content from files referenced with @filepath patterns.
    /// Returns the modified input with file contents appended, or None if no valid files found.
    pub async fn inject_at_files(input: &str) -> Option<String> {
        // Match @filepath patterns
        let re = Regex::new(r"@([^\s]+)").ok()?;
        let mut file_contents = Vec::new();

        for mat in re.find_iter(input) {
            let file_path = &mat.as_str()[1..]; // Remove the @ prefix
            let path = Path::new(file_path);

            if path.exists() && path.is_file() {
                if let Ok(file_content) = Self::format_file_content(path).await {
                    file_contents.push(file_content);
                }
            }
        }

        if !file_contents.is_empty() {
            let mut result = input.to_string();

            result.push_str("\n\n--- Content from referenced files ---\n");
            for content in file_contents {
                result.push('\n');
                result.push_str(&content);
            }
            result.push_str("\n--- End of content ---");

            Some(result)
        } else {
            None
        }
    }

    async fn format_file_content(path: &Path) -> Result<String, std::io::Error> {
        let content = tokio::fs::read_to_string(path).await?;

        let formatted_content = format!("Content from @{}:\n{}", path.to_string_lossy(), content.trim());

        Ok(formatted_content)
    }
}