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
    const EMPTY_FILE_PATH: &str = "/empty_file.txt";
    const LARGE_LINE_COUNT_FILE_PATH: &str = "/large_line_count.txt";

    /// Sets up the following filesystem structure:
    /// ```text
    /// test_file.txt
    /// test_file2.txt
    /// test_file3.txt (doesn't exist)
    /// empty_file.txt (exists but empty)
    /// large_line_count.txt (100 lines)
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
        
        // Create an empty file for edge case testing
        fs.write(EMPTY_FILE_PATH, "").await.unwrap();
        
        // Create a file with many lines for testing line number handling
        let mut large_file_content = String::new();
        for i in 1..=100 {
            large_file_content.push_str(&format!("Line {}: This is line number {}\n", i, i));
        }
        fs.write(LARGE_LINE_COUNT_FILE_PATH, large_file_content).await.unwrap();
        
        ctx
    }

    #[test]
    fn test_negative_index_conversion() {
        assert_eq!(convert_negative_index(5, -100), 0);
        assert_eq!(convert_negative_index(5, -1), 4);
        assert_eq!(convert_negative_index(5, 0), 0); // Edge case: 0 should be treated as first line
        assert_eq!(convert_negative_index(5, 1), 0); // 1-based to 0-based conversion
        assert_eq!(convert_negative_index(5, 5), 4); // Last line
        assert_eq!(convert_negative_index(5, 6), 5); // Beyond last line (will be clamped later)
    }

    #[tokio::test]
    async fn test_fs_read_line_edge_cases() {
        let ctx = setup_test_directory().await;
        let mut stdout = std::io::stdout();

        // Test empty file
        let v = serde_json::json!({
            "path": EMPTY_FILE_PATH,
            "mode": "Line",
        });
        let output = serde_json::from_value::<FsRead>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Text(text) = output.output {
            assert_eq!(text, "", "Empty file should return empty string");
        } else {
            panic!("expected text output");
        }

        // Test reading beyond file end
        let v = serde_json::json!({
            "path": TEST_FILE_PATH,
            "mode": "Line",
            "start_line": 10, // Beyond file end
            "end_line": 20,
        });
        let result = serde_json::from_value::<FsRead>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await;
        assert!(result.is_err(), "Reading beyond file end should return error");

        // Test reading with end_line before start_line (should adjust end to match start)
        let v = serde_json::json!({
            "path": TEST_FILE_PATH,
            "mode": "Line",
            "start_line": 3,
            "end_line": 2,
        });
        let output = serde_json::from_value::<FsRead>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Text(text) = output.output {
            assert_eq!(text, "3: asdf", "Should return just line 3 when end < start");
        } else {
            panic!("expected text output");
        }
    }

    #[tokio::test]
    async fn test_fs_read_search_line_numbers() {
        let ctx = setup_test_directory().await;
        let mut stdout = std::io::stdout();

        // Test search with pattern that appears on specific lines
        let v = serde_json::json!({
            "mode": "Search",
            "path": TEST_FILE_PATH,
            "pattern": "Hello",
            "context_lines": 0, // No context lines to simplify test
        });
        
        let output = serde_json::from_value::<FsRead>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Text(text) = output.output {
            let matches: Vec<SearchMatch> = serde_json::from_str(&text).unwrap();
            assert_eq!(matches.len(), 2, "Should find 2 matches for 'Hello'");
            assert_eq!(matches[0].line_number, 1, "First match should be on line 1");
            assert_eq!(matches[1].line_number, 4, "Second match should be on line 4");
        } else {
            panic!("expected text output");
        }

        // Test search with context lines
        let v = serde_json::json!({
            "mode": "Search",
            "path": LARGE_LINE_COUNT_FILE_PATH,
            "pattern": "Line 50",
            "context_lines": 2,
        });
        
        let output = serde_json::from_value::<FsRead>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Text(text) = output.output {
            let matches: Vec<SearchMatch> = serde_json::from_str(&text).unwrap();
            assert_eq!(matches.len(), 1, "Should find 1 match for 'Line 50'");
            assert_eq!(matches[0].line_number, 50, "Match should be on line 50");
            
            // Check that context includes correct line numbers
            let context = &matches[0].context;
            assert!(context.contains("48:"), "Context should include line 48");
            assert!(context.contains("49:"), "Context should include line 49");
            assert!(context.contains("50:"), "Context should include line 50 (match)");
            assert!(context.contains("51:"), "Context should include line 51");
            assert!(context.contains("52:"), "Context should include line 52");
        } else {
            panic!("expected text output");
        }
    }

    #[tokio::test]
    async fn test_fs_read_operations_structure() {
        let ctx = setup_test_directory().await;
        let mut stdout = std::io::stdout();

        // Test operations structure with multiple operations
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
                    "path": TEST_FILE2_PATH,
                    "pattern": "second"
                }
            ]
        });
        
        let output = serde_json::from_value::<FsRead>(v)
            .unwrap()
            .invoke(&ctx, &mut stdout)
            .await
            .unwrap();

        if let OutputKind::Text(text) = output.output {
            let batch_result: BatchReadResult = serde_json::from_str(&text).unwrap();
            
            assert_eq!(batch_result.total_files, 2, "Should have 2 operations");
            assert_eq!(batch_result.successful_reads, 2, "Both operations should succeed");
            assert_eq!(batch_result.failed_reads, 0, "No operations should fail");
            
            // Check first operation result (Line mode)
            assert_eq!(batch_result.results[0].path, TEST_FILE_PATH);
            assert!(batch_result.results[0].success);
            assert_eq!(batch_result.results[0].content, Some("1: Hello world!\n2: This is line 2".to_string()));
            assert!(batch_result.results[0].content_hash.is_some(), "Should include content hash");
            assert!(batch_result.results[0].last_modified.is_some(), "Should include last_modified timestamp");
            
            // Check second operation result (Search mode)
            assert_eq!(batch_result.results[1].path, TEST_FILE2_PATH);
            assert!(batch_result.results[1].success);
            assert!(batch_result.results[1].content.is_some(), "Search result should have content");
            
            // Verify search results can be parsed from the content
            let search_matches: Vec<SearchMatch> = serde_json::from_str(batch_result.results[1].content.as_ref().unwrap()).unwrap();
            assert_eq!(search_matches.len(), 1, "Should find 1 match for 'second'");
            assert_eq!(search_matches[0].line_number, 1, "Match should be on line 1");
        } else {
            panic!("expected text output");
        }
    }

    #[tokio::test]
    async fn test_fs_read_line_invoke() {
        let ctx = setup_test_directory().await;
        let lines = TEST_FILE_CONTENTS.lines().collect::<Vec<_>>();
        let mut stdout = std::io::stdout();

        macro_rules! assert_lines {
            ($start_line:expr, $end_line:expr, $expected:expr) => {
                let v = serde_json::json!({
                    "path": TEST_FILE_PATH,
                    "mode": "Line",
                    "start_line": $start_line,
                    "end_line": $end_line,
                });
                let fs_read = serde_json::from_value::<FsRead>(v).unwrap();
                let output = fs_read.invoke(&ctx, &mut stdout).await.unwrap();

                if let OutputKind::Text(text) = output.output {
                    assert_eq!(text, $expected.join("\n"), "actual(left) does not equal
                                expected(right) for (start_line, end_line): ({:?}, {:?})", $start_line, $end_line);
                } else {
                    panic!("expected text output");
                }
            }
        }
        assert_lines!(None::<i32>, None::<i32>, lines[..]);
        assert_lines!(1, 2, lines[..=1]);
        assert_lines!(1, -1, lines[..]);
        assert_lines!(2, 1, lines[1..=1]); // End < start should return just start line
        assert_lines!(-2, -1, lines[2..]);
        assert_lines!(-2, None::<i32>, lines[2..]);
        assert_lines!(2, None::<i32>, lines[1..]);
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
