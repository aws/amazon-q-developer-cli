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
    fn test_fs_read_operations_deser() {
        // Test operations deserialization
        let v = serde_json::json!({
            "operations": [
                {
                    "mode": "Line",
                    "path": "/test_file.txt",
                    "start_line": 1,
                    "end_line": 2
                },
                {
                    "mode": "Line",
                    "path": "/test_file2.txt",
                    "start_line": 1,
                    "end_line": 3
                }
            ]
        });
        
        let fs_read = serde_json::from_value::<FsRead>(v).unwrap();
        match fs_read {
            FsRead::Operations(ops) => {
                assert_eq!(ops.operations.len(), 2);
                match &ops.operations[0] {
                    FsReadOperation::Line(op) => {
                        assert_eq!(op.path, "/test_file.txt");
                        assert_eq!(op.start_line, Some(1));
                        assert_eq!(op.end_line, Some(2));
                    },
                    _ => panic!("Expected Line operation"),
                }
                match &ops.operations[1] {
                    FsReadOperation::Line(op) => {
                        assert_eq!(op.path, "/test_file2.txt");
                        assert_eq!(op.start_line, Some(1));
                        assert_eq!(op.end_line, Some(3));
                    },
                    _ => panic!("Expected Line operation"),
                }
            },
            _ => panic!("Expected Operations variant"),
        }
    }

    #[test]
    fn test_fs_read_mixed_operations_deser() {
        // Test mixed operations deserialization
        let v = serde_json::json!({
            "operations": [
                {
                    "mode": "Line",
                    "path": "/test_file.txt",
                    "start_line": 1,
                    "end_line": 2
                },
                {
                    "mode": "Search",
                    "path": "/test_file2.txt",
                    "pattern": "hello"
                },
                {
                    "mode": "Directory",
                    "path": "/",
                    "depth": 1
                }
            ]
        });
        
        let fs_read = serde_json::from_value::<FsRead>(v).unwrap();
        match fs_read {
            FsRead::Operations(ops) => {
                assert_eq!(ops.operations.len(), 3);
                match &ops.operations[0] {
                    FsReadOperation::Line(_) => {},
                    _ => panic!("Expected Line operation"),
                }
                match &ops.operations[1] {
                    FsReadOperation::Search(_) => {},
                    _ => panic!("Expected Search operation"),
                }
                match &ops.operations[2] {
                    FsReadOperation::Directory(_) => {},
                    _ => panic!("Expected Directory operation"),
                }
            },
            _ => panic!("Expected Operations variant"),
        }
    }

    #[tokio::test]
    async fn test_fs_read_operations_invoke() {
        let ctx = setup_test_directory().await;
        let mut stdout = std::io::stdout();

        // Test operations with multiple files
        let v = serde_json::json!({
            "operations": [
                {
                    "mode": "Line",
                    "path": TEST_FILE_PATH,
                    "start_line": 1,
                    "end_line": 2
                },
                {
                    "mode": "Line",
                    "path": TEST_FILE2_PATH,
                    "start_line": 1,
                    "end_line": 1
                }
            ]
        });

        let output = serde_json::from_value::<FsRead>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Text(text) = output.output {
            let results: Vec<FileReadResult> = serde_json::from_str(&text).unwrap();
            assert_eq!(results.len(), 2);

            // Check first file
            assert_eq!(results[0].path, TEST_FILE_PATH);
            assert!(results[0].success);
            assert_eq!(results[0].content, Some("1: Hello world!\n2: This is line 2".to_string()));
            assert_eq!(results[0].error, None);

            // Check second file
            assert_eq!(results[1].path, TEST_FILE2_PATH);
            assert!(results[1].success);
            assert_eq!(
                results[1].content,
                Some("This is the second test file".to_string())
            );
            assert_eq!(results[1].error, None);
        } else {
            panic!("expected text output");
        }
    }

    #[tokio::test]
    async fn test_fs_read_operations_mixed_invoke() {
        let ctx = setup_test_directory().await;
        let mut stdout = std::io::stdout();

        // Test mixed operations
        let v = serde_json::json!({
            "operations": [
                {
                    "mode": "Line",
                    "path": TEST_FILE_PATH,
                    "start_line": 1,
                    "end_line": 2
                },
                {
                    "mode": "Search",
                    "path": TEST_FILE_PATH,
                    "pattern": "hello"
                }
            ]
        });

        let output = serde_json::from_value::<FsRead>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Text(text) = output.output {
            let results: Vec<FileReadResult> = serde_json::from_str(&text).unwrap();
            assert_eq!(results.len(), 2);

            // Check first result (Line operation)
            assert_eq!(results[0].path, TEST_FILE_PATH);
            assert!(results[0].success);
            assert_eq!(results[0].content, Some("1: Hello world!\n2: This is line 2".to_string()));
            
            // Check second result (Search operation)
            assert_eq!(results[1].path, TEST_FILE_PATH);
            assert!(results[1].success);
            assert!(results[1].content.is_some());
            
            // Parse search results from content
            let search_results: Vec<SearchMatch> = serde_json::from_str(results[1].content.as_ref().unwrap()).unwrap();
            assert_eq!(search_results.len(), 2); // "hello" appears twice in the test file
        } else {
            panic!("expected text output");
        }
    }

    #[tokio::test]
    async fn test_fs_read_operations_with_error() {
        let ctx = setup_test_directory().await;
        let mut stdout = std::io::stdout();

        // Test operations with one file that doesn't exist
        let v = serde_json::json!({
            "operations": [
                {
                    "mode": "Line",
                    "path": TEST_FILE_PATH,
                    "start_line": 1,
                    "end_line": 2
                },
                {
                    "mode": "Line",
                    "path": TEST_FILE3_PATH, // This file doesn't exist
                    "start_line": 1,
                    "end_line": 1
                }
            ]
        });

        // Validation should fail
        let mut fs_read = serde_json::from_value::<FsRead>(v).unwrap();
        assert!(fs_read.validate(&ctx).await.is_err());
    }

    #[tokio::test]
    async fn test_fs_read_operations_single_result() {
        let ctx = setup_test_directory().await;
        let mut stdout = std::io::stdout();

        // Test operations with a single file
        let v = serde_json::json!({
            "operations": [
                {
                    "mode": "Line",
                    "path": TEST_FILE_PATH,
                    "start_line": 1,
                    "end_line": 2
                }
            ]
        });

        let output = serde_json::from_value::<FsRead>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Text(text) = output.output {
            // For a single operation, the result should be the content directly
            assert_eq!(text, "1: Hello world!\n2: This is line 2");
        } else {
            panic!("expected text output");
        }
    }
}
