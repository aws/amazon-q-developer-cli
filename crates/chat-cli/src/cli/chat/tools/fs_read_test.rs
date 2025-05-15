#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    const TEST_FILE_CONTENTS: &str = "\
1: Hello world!
2: This is line 2
3: asdf
4: Hello world!
";

    const TEST_FILE_PATH: &str = "/test_file.txt";
    const TEST_FILE2_PATH: &str = "/test_file2.txt";
    const TEST_FILE3_PATH: &str = "/test_file3.txt";
    const TEST_HIDDEN_FILE_PATH: &str = "/aaaa2/.hidden";

    /// Sets up the following filesystem structure:
    /// ```text
    /// test_file.txt
    /// test_file2.txt
    /// test_file3.txt (doesn't exist)
    /// /home/testuser/
    /// /aaaa1/
    ///     /bbbb1/
    ///         /cccc1/
    /// /aaaa2/
    ///     .hidden
    /// ```
    async fn setup_test_directory() -> Arc<Context> {
        let ctx = Context::builder().with_test_home().await.unwrap().build_fake();
        let fs = ctx.fs();
        fs.write(TEST_FILE_PATH, TEST_FILE_CONTENTS).await.unwrap();
        fs.write(TEST_FILE2_PATH, "This is the second test file\nWith multiple lines")
            .await
            .unwrap();
        fs.create_dir_all("/aaaa1/bbbb1/cccc1").await.unwrap();
        fs.create_dir_all("/aaaa2").await.unwrap();
        fs.write(TEST_HIDDEN_FILE_PATH, "this is a hidden file").await.unwrap();
        ctx
    }

    #[test]
    fn test_negative_index_conversion() {
        assert_eq!(convert_negative_index(5, -100), 0);
        assert_eq!(convert_negative_index(5, -1), 4);
    }

    #[test]
    fn test_format_mode() {
        macro_rules! assert_mode {
            ($actual:expr, $expected:expr) => {
                assert_eq!(format_mode($actual).iter().collect::<String>(), $expected);
            };
        }
        assert_mode!(0o000, "---------");
        assert_mode!(0o700, "rwx------");
        assert_mode!(0o744, "rwxr--r--");
        assert_mode!(0o641, "rw-r----x");
    }

    #[test]
    fn test_path_or_paths() {
        // Test single path
        let single = PathOrPaths::Single("test.txt".to_string());
        assert!(!single.is_batch());
        assert_eq!(single.as_single(), Some(&"test.txt".to_string()));
        assert_eq!(single.as_multiple(), None);

        let paths: Vec<String> = single.iter().cloned().collect();
        assert_eq!(paths, vec!["test.txt".to_string()]);

        // Test multiple paths
        let multiple = PathOrPaths::Multiple(vec!["test1.txt".to_string(), "test2.txt".to_string()]);
        assert!(multiple.is_batch());
        assert_eq!(multiple.as_single(), None);
        assert_eq!(
            multiple.as_multiple(),
            Some(&vec!["test1.txt".to_string(), "test2.txt".to_string()])
        );

        let paths: Vec<String> = multiple.iter().cloned().collect();
        assert_eq!(paths, vec!["test1.txt".to_string(), "test2.txt".to_string()]);
    }

    #[test]
    fn test_deserialize_path_or_paths() {
        // Test deserializing a string to a single path
        let json = r#""test.txt""#;
        let path_or_paths: PathOrPaths = serde_json::from_str(json).unwrap();
        assert!(!path_or_paths.is_batch());
        assert_eq!(path_or_paths.as_single(), Some(&"test.txt".to_string()));

        // Test deserializing an array to multiple paths
        let json = r#"["test1.txt", "test2.txt"]"#;
        let path_or_paths: PathOrPaths = serde_json::from_str(json).unwrap();
        assert!(path_or_paths.is_batch());
        assert_eq!(
            path_or_paths.as_multiple(),
            Some(&vec!["test1.txt".to_string(), "test2.txt".to_string()])
        );
    }
}
