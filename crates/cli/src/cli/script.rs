use std::io;

use eyre::Result;
use tokio::fs;

/// read_q_script reads the file passed in and returns its contents. If it's first line begins with
/// a shebang (#!), that line and consecutive lines beginning with a '#' character are skipped.
pub async fn read_q_script(script: &str) -> Result<String, io::Error> {
    match fs::read_to_string(&script).await {
        Ok(content) => {
            let mut lines = content.lines().peekable();
            let mut result_lines = Vec::new();

            // Only skip the first line if it starts with '#!'
            if lines.peek().is_some_and(|line| line.starts_with("#!")) {
                lines.next();
                // Skip consecutive comment lines after shebang
                while lines.peek().is_some_and(|line| line.starts_with('#')) {
                    lines.next();
                }
            }

            result_lines.extend(lines);
            Ok(result_lines.join("\n"))
        },
        Err(e) => Err(e),
    }
}
#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;

    fn create_temp_script(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        file.write_all(content.as_bytes()).unwrap();
        file
    }

    #[tokio::test]
    async fn test_script_with_shebang_and_consecutive_comments() {
        let content =
            "#!/usr/bin/env q\n# This is a comment\n# Another comment\nactual content\n# Later comment\nmore content";
        let file = create_temp_script(content);

        let result = read_q_script(file.path().to_string_lossy().as_ref()).await;
        assert_eq!(result.unwrap(), "actual content\n# Later comment\nmore content");
    }

    #[tokio::test]
    async fn test_script_with_shebang_no_comments() {
        let content = "#!/usr/bin/env q\nactual content\nmore content";
        let file = create_temp_script(content);

        let result = read_q_script(file.path().to_string_lossy().as_ref()).await;
        assert_eq!(result.unwrap(), "actual content\nmore content");
    }

    #[tokio::test]
    async fn test_script_without_shebang_with_comments() {
        let content = "# This is a comment\nactual content\n# Later comment\nmore content";
        let file = create_temp_script(content);

        let result = read_q_script(file.path().to_string_lossy().as_ref()).await;
        assert_eq!(
            result.unwrap(),
            "# This is a comment\nactual content\n# Later comment\nmore content"
        );
    }

    #[tokio::test]
    async fn test_script_without_shebang_no_comments() {
        let content = "actual content\nmore content";
        let file = create_temp_script(content);

        let result = read_q_script(file.path().to_string_lossy().as_ref()).await;
        assert_eq!(result.unwrap(), "actual content\nmore content");
    }

    #[tokio::test]
    async fn test_script_with_only_shebang() {
        let content = "#!/usr/bin/env q";
        let file = create_temp_script(content);

        let result = read_q_script(file.path().to_string_lossy().as_ref()).await;
        assert_eq!(result.unwrap(), "");
    }

    #[tokio::test]
    async fn test_script_with_shebang_and_only_comments() {
        let content = "#!/usr/bin/env q\n# Comment 1\n# Comment 2";
        let file = create_temp_script(content);

        let result = read_q_script(file.path().to_string_lossy().as_ref()).await;
        assert_eq!(result.unwrap(), "");
    }

    #[tokio::test]
    async fn test_script_with_non_shebang_hash_first_line() {
        let content = "#not a shebang\nactual content\nmore content";
        let file = create_temp_script(content);

        let result = read_q_script(file.path().to_string_lossy().as_ref()).await;
        assert_eq!(result.unwrap(), "#not a shebang\nactual content\nmore content");
    }

    #[tokio::test]
    async fn test_empty_script() {
        let content = "";
        let file = create_temp_script(content);

        let result = read_q_script(file.path().to_string_lossy().as_ref()).await;
        assert_eq!(result.unwrap(), "");
    }

    #[tokio::test]
    async fn test_script_with_mixed_content() {
        let content = "#!/usr/bin/env q\n# Header comment\n# Another header comment\n\nactual content\n# Inline comment\nmore content\n\n# Final comment";
        let file = create_temp_script(content);

        let result = read_q_script(file.path().to_string_lossy().as_ref()).await;
        assert_eq!(
            result.unwrap(),
            "\nactual content\n# Inline comment\nmore content\n\n# Final comment"
        );
    }

    #[tokio::test]
    async fn test_nonexistent_file() {
        let result = read_q_script("/nonexistent/file.q").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_script_with_shebang_and_empty_lines() {
        let content = "#!/usr/bin/env q\n# Comment\n\n\nactual content";
        let file = create_temp_script(content);

        let result = read_q_script(file.path().to_string_lossy().as_ref()).await;
        assert_eq!(result.unwrap(), "\n\nactual content");
    }
}
