use crate::database::{Database, settings::Setting};

/// Truncates a command for display purposes based on the ChatHistoryMaxLength setting.
///
/// This function reads the current setting value from the database and applies truncation
/// logic accordingly. Commands are truncated at word boundaries to avoid cutting words in half.
/// If no word boundary is found, it falls back to character boundary truncation.
///
/// # Arguments
/// * `command` - The command string to potentially truncate
/// * `database` - Database instance to read the setting from
///
/// # Returns
/// * For setting > 0: Returns the command truncated at word boundaries with "..." appended if truncated
/// * For setting = 0: Returns empty string (history hidden mode)
/// * For invalid/missing setting: Uses default value of 80 characters
///
/// # Examples
/// ```ignore
/// let db = Database::new().await.unwrap();
/// let result = truncate_command_for_display("short command", &db);
/// // Returns "short command" if setting allows
///
/// let long_cmd = "this is a very long command that exceeds the limit";
/// let result = truncate_command_for_display(&long_cmd, &db);
/// // Returns "this is a very long command that exceeds..." (truncated at word boundary)
/// ```
pub fn truncate_command_for_display(command: &str, database: &Database) -> String {
    let max_length = database.settings.get_int(Setting::ChatHistoryMaxLength).unwrap_or(80);

    // Handle invalid values (negative) by using default
    let max_length = if max_length < 0 { 80 } else { max_length };

    if max_length == 0 {
        // History hidden mode - return empty string for display
        return String::new();
    }

    if command.len() <= max_length as usize {
        // Command fits within limit
        return command.to_string();
    }

    let limit = max_length as usize;

    // First, find a safe character boundary within the limit
    let mut safe_limit = 0;
    let mut char_count = 0;

    for (byte_index, _) in command.char_indices() {
        if char_count >= limit {
            break;
        }
        safe_limit = byte_index;
        char_count += 1;
    }

    // If we haven't reached the limit in characters, use the full string
    if char_count < limit {
        safe_limit = command.len();
    }

    // Now try to find the last word boundary (whitespace) before the safe limit
    if let Some(last_space) = command[..safe_limit].rfind(char::is_whitespace) {
        // Found a word boundary, truncate there
        let truncated = command[..last_space].trim_end();
        if !truncated.is_empty() {
            return format!("{}...", truncated);
        }
    }

    // No suitable word boundary found, or the result would be empty
    // Fall back to character boundary truncation
    let truncate_at = limit.saturating_sub(3); // Reserve space for "..."
    let mut char_boundary = 0;
    let mut final_char_count = 0;

    for (byte_index, _) in command.char_indices() {
        if final_char_count >= truncate_at {
            break;
        }
        char_boundary = byte_index;
        final_char_count += 1;
    }

    // If we haven't reached the truncation point, use the full string length
    if final_char_count < truncate_at {
        char_boundary = command.len();
    }

    format!("{}...", &command[..char_boundary])
}

/// Determines whether command history should be displayed based on the ChatHistoryMaxLength setting.
///
/// This helper function checks if the setting allows history display. When the setting is 0,
/// history should be stored but not displayed to the user.
///
/// # Arguments
/// * `database` - Database instance to read the setting from
///
/// # Returns
/// * `true` if history should be displayed (setting > 0 or default)
/// * `false` if history should be hidden (setting = 0)
///
/// # Examples
/// ```ignore
/// let db = Database::new().await.unwrap();
/// if should_display_history(&db) {
///     // Show history entries to user
/// } else {
///     // Hide history from display but continue storing
/// }
/// ```
pub fn should_display_history(database: &Database) -> bool {
    let max_length = database.settings.get_int(Setting::ChatHistoryMaxLength).unwrap_or(80);

    // Handle invalid values (negative) by using default
    let max_length = if max_length < 0 { 80 } else { max_length };

    max_length != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use crate::database::settings::Setting;
    use tokio::time::{Duration, sleep};

    #[tokio::test]
    async fn test_truncate_command_for_display_default_setting() {
        let db = Database::new().await.unwrap();

        // Test short command (should not be truncated)
        let short_cmd = "echo hello";
        let result = truncate_command_for_display(short_cmd, &db);
        assert_eq!(result, "echo hello");

        // Test command exactly at default limit (80 chars)
        let exact_cmd = "a".repeat(80);
        let result = truncate_command_for_display(&exact_cmd, &db);
        assert_eq!(result, exact_cmd);

        // Test command over default limit with word boundaries
        let long_cmd =
            "this is a very long command that definitely exceeds the eighty character limit and should be truncated";
        let result = truncate_command_for_display(&long_cmd, &db);
        // Should truncate at word boundary before 80 chars
        assert!(result.ends_with("..."));
        // The result should be shorter than the original and end with ...
        assert!(result.len() < long_cmd.len());
        // Should truncate at a reasonable word boundary
        assert!(result.starts_with("this is a very long command"));
    }

    #[tokio::test]
    async fn test_truncate_command_for_display_custom_setting() {
        let mut db = Database::new().await.unwrap();

        // Set custom length
        db.settings.set(Setting::ChatHistoryMaxLength, 20).await.unwrap();

        let long_cmd = "this is a very long command that exceeds 20 characters";
        let result = truncate_command_for_display(long_cmd, &db);
        assert!(result.ends_with("..."));
        // Should be truncated and shorter than original
        assert!(result.len() < long_cmd.len());
        // Should truncate at word boundary
        assert!(result.starts_with("this is"));

        // Test command exactly at custom limit
        let exact_cmd = "a".repeat(20);
        let result = truncate_command_for_display(&exact_cmd, &db);
        assert_eq!(result, exact_cmd);
    }

    #[tokio::test]
    async fn test_truncate_command_for_display_zero_setting() {
        let mut db = Database::new().await.unwrap();

        // Set to zero (hidden mode)
        db.settings.set(Setting::ChatHistoryMaxLength, 0).await.unwrap();

        let cmd = "any command";
        let result = truncate_command_for_display(cmd, &db);
        assert_eq!(result, "");
    }

    #[tokio::test]
    async fn test_truncate_command_for_display_negative_setting() {
        let mut db = Database::new().await.unwrap();

        // Set negative value (should use default)
        db.settings.set(Setting::ChatHistoryMaxLength, -10).await.unwrap();

        let long_cmd =
            "this is a very long command that definitely exceeds the eighty character limit and should be truncated";
        let result = truncate_command_for_display(&long_cmd, &db);
        assert!(result.ends_with("..."));
        // Should be truncated and shorter than original
        assert!(result.len() < long_cmd.len());
    }

    #[tokio::test]
    async fn test_truncate_command_for_display_word_boundaries() {
        let mut db = Database::new().await.unwrap();

        // Set a limit that will test word boundary logic
        db.settings.set(Setting::ChatHistoryMaxLength, 25).await.unwrap();

        // Test with clear word boundaries
        let cmd = "short words here and more";
        let result = truncate_command_for_display(cmd, &db);
        if result.len() < cmd.len() {
            assert!(result.ends_with("..."));
            // Should not cut words in half
            assert!(result.starts_with("short words"));
        }

        // Test with no spaces (should fall back to char boundary)
        let no_spaces = "verylongcommandwithoutanyspaces";
        let result = truncate_command_for_display(no_spaces, &db);
        assert!(result.ends_with("..."));
        // Should be truncated to fit within limit
        assert!(result.len() <= 28); // 25 + "..." = 28
    }

    #[tokio::test]
    async fn test_truncate_command_for_display_unicode_safety() {
        let mut db = Database::new().await.unwrap();

        // Set a small limit to test unicode boundary handling
        db.settings.set(Setting::ChatHistoryMaxLength, 15).await.unwrap();

        // Test with unicode characters and word boundaries
        let unicode_cmd = "héllo wörld 🌍 test";
        let result = truncate_command_for_display(unicode_cmd, &db);
        assert!(result.ends_with("..."));
        // Should truncate at word boundary, likely "héllo wörld..."
        assert!(result.starts_with("héllo"));

        // Test with emoji at boundary - no spaces
        let emoji_cmd = "test🌍🌍🌍🌍🌍🌍🌍🌍";
        let result = truncate_command_for_display(emoji_cmd, &db);
        assert!(result.ends_with("..."));
        // Should handle unicode properly in fallback mode
    }

    #[tokio::test]
    async fn test_truncate_command_for_display_edge_cases() {
        let db = Database::new().await.unwrap();

        // Test empty string
        let result = truncate_command_for_display("", &db);
        assert_eq!(result, "");

        // Test whitespace only
        let result = truncate_command_for_display("   ", &db);
        assert_eq!(result, "   ");

        // Test single character
        let result = truncate_command_for_display("a", &db);
        assert_eq!(result, "a");

        // Test string that's all spaces up to limit
        let mut db = Database::new().await.unwrap();
        db.settings.set(Setting::ChatHistoryMaxLength, 10).await.unwrap();
        let spaces = " ".repeat(15);
        let result = truncate_command_for_display(&spaces, &db);
        // Should handle gracefully, likely fall back to char truncation
        if result.len() < spaces.len() {
            assert!(result.ends_with("..."));
        }
    }

    #[tokio::test]
    async fn test_should_display_history_default() {
        let db = Database::new().await.unwrap();

        // Default should be true (80 > 0)
        assert!(should_display_history(&db));
    }

    #[tokio::test]
    async fn test_should_display_history_custom_positive() {
        let mut db = Database::new().await.unwrap();

        db.settings.set(Setting::ChatHistoryMaxLength, 100).await.unwrap();
        assert!(should_display_history(&db));

        db.settings.set(Setting::ChatHistoryMaxLength, 1).await.unwrap();
        assert!(should_display_history(&db));
    }

    #[tokio::test]
    async fn test_should_display_history_zero() {
        let mut db = Database::new().await.unwrap();

        db.settings.set(Setting::ChatHistoryMaxLength, 0).await.unwrap();
        assert!(!should_display_history(&db));
    }

    #[tokio::test]
    async fn test_should_display_history_negative() {
        let mut db = Database::new().await.unwrap();

        // Negative values should fall back to default (true)
        db.settings.set(Setting::ChatHistoryMaxLength, -5).await.unwrap();
        assert!(should_display_history(&db));
    }

    #[tokio::test]
    async fn test_truncate_command_consistency() {
        let mut db = Database::new().await.unwrap();

        // Test that both functions handle settings consistently
        db.settings.set(Setting::ChatHistoryMaxLength, 0).await.unwrap();

        assert!(!should_display_history(&db));
        assert_eq!(truncate_command_for_display("any command", &db), "");

        db.settings.set(Setting::ChatHistoryMaxLength, 15).await.unwrap();

        assert!(should_display_history(&db));
        let result = truncate_command_for_display("this is a long command", &db);
        assert!(result.ends_with("..."));
        assert!(result.len() < "this is a long command".len());
    }

    #[tokio::test]
    async fn test_truncate_command_boundary_cases() {
        let mut db = Database::new().await.unwrap();

        // Test exactly at boundary
        db.settings.set(Setting::ChatHistoryMaxLength, 10).await.unwrap();

        let cmd_10 = "1234567890";
        let result = truncate_command_for_display(cmd_10, &db);
        assert_eq!(result, "1234567890");

        let cmd_11 = "12345678901";
        let result = truncate_command_for_display(cmd_11, &db);
        assert!(result.ends_with("..."));
        // Should be truncated to fit within the limit
        assert!(result.len() <= 13); // 10 + "..." = 13

        // Test with word boundary exactly at limit
        let cmd_with_space = "hello worl"; // 10 chars
        let result = truncate_command_for_display(cmd_with_space, &db);
        assert_eq!(result, "hello worl");

        let cmd_with_space_over = "hello world"; // 11 chars
        let result = truncate_command_for_display(cmd_with_space_over, &db);
        assert!(result.ends_with("..."));
        // Should truncate at word boundary, likely "hello..."
        assert!(result.starts_with("hello"));
    }

    // Integration tests for comprehensive command history truncation feature

    #[tokio::test]
    async fn test_integration_setting_changes_affect_display_immediately() {
        // Create a database for testing
        let mut db = Database::new().await.unwrap();

        let long_command = "this is a very long command that definitely exceeds the default eighty character limit and should be truncated properly";

        // Test with default setting (80)
        let result = truncate_command_for_display(&long_command, &db);
        assert!(result.ends_with("..."));
        assert!(result.len() < long_command.len());
        assert!(should_display_history(&db));

        // Change setting to 30 and verify immediate effect
        db.settings.set(Setting::ChatHistoryMaxLength, 30).await.unwrap();
        let result_30 = truncate_command_for_display(&long_command, &db);
        assert!(result_30.ends_with("..."));
        assert!(result_30.len() < result.len()); // Should be shorter than 80-char version
        assert!(should_display_history(&db));

        // Change setting to 100 and verify immediate effect
        db.settings.set(Setting::ChatHistoryMaxLength, 100).await.unwrap();
        let result_100 = truncate_command_for_display(&long_command, &db);
        assert!(result_100.ends_with("..."));
        assert!(result_100.len() > result.len()); // Should be longer than 80-char version
        assert!(should_display_history(&db));

        // Change setting to 0 (hidden) and verify immediate effect
        db.settings.set(Setting::ChatHistoryMaxLength, 0).await.unwrap();
        let result_hidden = truncate_command_for_display(&long_command, &db);
        assert_eq!(result_hidden, "");
        assert!(!should_display_history(&db));

        // Change back to positive value and verify it works again
        db.settings.set(Setting::ChatHistoryMaxLength, 40).await.unwrap();
        let result_40 = truncate_command_for_display(&long_command, &db);
        assert!(result_40.ends_with("..."));
        assert!(!result_40.is_empty());
        assert!(should_display_history(&db));
    }

    #[tokio::test]
    async fn test_integration_history_storage_vs_display_with_zero_setting() {
        let mut db = Database::new().await.unwrap();

        // Set history display to hidden (0)
        db.settings.set(Setting::ChatHistoryMaxLength, 0).await.unwrap();

        let commands = vec![
            "echo hello world",
            "ls -la /some/very/long/path/that/exceeds/normal/limits",
            "git commit -m 'this is a very long commit message that describes many changes'",
        ];

        // Verify that history display is disabled
        assert!(!should_display_history(&db));

        // Verify that truncation returns empty string for display
        for cmd in &commands {
            let display_result = truncate_command_for_display(cmd, &db);
            assert_eq!(display_result, "");
        }

        // Now enable history display and verify commands can be displayed (truncated)
        db.settings.set(Setting::ChatHistoryMaxLength, 50).await.unwrap();

        assert!(should_display_history(&db));

        // Verify that commands can now be displayed (truncated)
        for cmd in &commands {
            let display_result = truncate_command_for_display(cmd, &db);
            if cmd.len() > 50 {
                assert!(display_result.ends_with("..."));
                assert!(display_result.len() <= 53); // 50 + "..." = 53
            } else {
                assert_eq!(display_result, *cmd);
            }
        }
    }

    #[tokio::test]
    async fn test_integration_invalid_setting_values_fallback_to_default() {
        let mut db = Database::new().await.unwrap();

        let test_command =
            "this is a test command that is longer than eighty characters and should be truncated properly";

        // Test negative values fall back to default (80)
        let negative_values = vec![-1, -10, -100, -999];
        for negative_val in negative_values {
            db.settings
                .set(Setting::ChatHistoryMaxLength, negative_val)
                .await
                .unwrap();

            let result = truncate_command_for_display(&test_command, &db);
            assert!(result.ends_with("..."));
            // Should behave like default (80)
            assert!(result.len() <= 83); // 80 + "..." = 83
            assert!(should_display_history(&db)); // Should display with default behavior
        }

        // Test very large values work correctly
        db.settings.set(Setting::ChatHistoryMaxLength, 1000).await.unwrap();
        let result_large = truncate_command_for_display(&test_command, &db);
        assert_eq!(result_large, test_command); // Should not be truncated
        assert!(should_display_history(&db));

        // Test boundary value (1)
        db.settings.set(Setting::ChatHistoryMaxLength, 1).await.unwrap();
        let result_one = truncate_command_for_display(&test_command, &db);
        assert!(result_one.ends_with("..."));
        assert!(result_one.len() <= 4); // 1 + "..." = 4
        assert!(should_display_history(&db));
    }

    #[tokio::test]
    async fn test_integration_unicode_character_handling_in_truncation() {
        let mut db = Database::new().await.unwrap();

        // Set a small limit to test unicode boundary handling
        db.settings.set(Setting::ChatHistoryMaxLength, 20).await.unwrap();

        // Test various unicode scenarios
        let unicode_tests = vec![
            // Basic unicode characters
            ("héllo wörld test command", "héllo wörld"),
            // Emoji characters
            ("test 🌍🌎🌏 command", "test 🌍🌎🌏"),
            // Mixed unicode and ASCII
            ("café résumé naïve command", "café résumé"),
            // Multi-byte characters
            ("こんにちは world test", "こんにちは world"),
            // Complex emoji sequences
            ("test 👨‍👩‍👧‍👦 family emoji", "test 👨‍👩‍👧‍👦"),
        ];

        for (input, expected_start) in unicode_tests {
            let result = truncate_command_for_display(input, &db);

            if input.len() > 20 {
                assert!(result.ends_with("..."));
                // Verify no unicode characters are broken
                assert!(result.is_char_boundary(result.len() - 3)); // Before "..."
                // Verify it starts with expected content
                assert!(result.starts_with(expected_start) || result.contains(expected_start));
            } else {
                assert_eq!(result, input);
            }

            // Verify the result is valid UTF-8
            assert!(std::str::from_utf8(result.as_bytes()).is_ok());
        }

        // Test edge case: command that's exactly at unicode boundary
        let boundary_test = "test🌍"; // 7 bytes but 5 characters
        db.settings.set(Setting::ChatHistoryMaxLength, 5).await.unwrap();
        let result = truncate_command_for_display(&boundary_test, &db);
        // The emoji might be counted differently, so just verify it handles unicode safely
        assert!(result == boundary_test || result.ends_with("..."));

        db.settings.set(Setting::ChatHistoryMaxLength, 4).await.unwrap();
        let result = truncate_command_for_display(&boundary_test, &db);
        assert!(result.ends_with("..."));
        // With very small limits, the result might be just "..." or have minimal content
        assert!(result.len() <= 7); // Should be reasonable length
    }

    #[tokio::test]
    async fn test_integration_very_long_commands_and_edge_cases() {
        let mut db = Database::new().await.unwrap();

        // Test extremely long command
        let very_long_command = "a".repeat(10000);
        db.settings.set(Setting::ChatHistoryMaxLength, 50).await.unwrap();

        let result = truncate_command_for_display(&very_long_command, &db);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 53); // 50 + "..." = 53

        // Test command with only spaces
        let spaces_command = " ".repeat(100);
        let result = truncate_command_for_display(&spaces_command, &db);
        if spaces_command.len() > 50 {
            assert!(result.ends_with("..."));
        }

        // Test empty command
        let result = truncate_command_for_display("", &db);
        assert_eq!(result, "");

        // Test single character
        let result = truncate_command_for_display("a", &db);
        assert_eq!(result, "a");

        // Test command with newlines and special characters
        let special_command = "echo 'hello\nworld'\t&& ls -la | grep test";
        let result = truncate_command_for_display(&special_command, &db);
        if special_command.len() > 50 {
            assert!(result.ends_with("..."));
            assert!(result.len() <= 53);
        } else {
            assert_eq!(result, special_command);
        }

        // Test word boundary truncation with very long words
        let no_spaces = "verylongcommandwithoutanyspacesorwordbreaksthatexceedsthelimit";
        let result = truncate_command_for_display(&no_spaces, &db);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 53);

        // Test command that's exactly at the limit
        let exact_limit = "a".repeat(50);
        let result = truncate_command_for_display(&exact_limit, &db);
        assert_eq!(result, exact_limit); // Should not be truncated

        // Test command that's one character over the limit
        let one_over = "a".repeat(51);
        let result = truncate_command_for_display(&one_over, &db);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 53);
    }

    #[tokio::test]
    async fn test_integration_concurrent_setting_changes() {
        let mut db = Database::new().await.unwrap();

        let test_command = "this is a test command for concurrent access testing";

        // Test rapid setting changes
        let settings_values = vec![10, 20, 30, 40, 50, 0, 80, 100];

        for &value in &settings_values {
            db.settings.set(Setting::ChatHistoryMaxLength, value).await.unwrap();

            let result = truncate_command_for_display(&test_command, &db);
            let should_display = should_display_history(&db);

            if value == 0 {
                assert_eq!(result, "");
                assert!(!should_display);
            } else {
                assert!(should_display);
                if test_command.len() > value as usize {
                    assert!(result.ends_with("..."));
                    assert!(result.len() <= (value as usize + 3));
                } else {
                    assert_eq!(result, test_command);
                }
            }

            // Small delay to simulate real usage
            sleep(Duration::from_millis(1)).await;
        }
    }

    #[tokio::test]
    async fn test_integration_word_boundary_truncation_edge_cases() {
        let mut db = Database::new().await.unwrap();

        db.settings.set(Setting::ChatHistoryMaxLength, 20).await.unwrap();

        // Test various word boundary scenarios
        let test_cases = vec![
            // Command with space exactly at limit
            ("hello world test cmd", "hello world"),
            // Command with no spaces (should fall back to char boundary)
            ("verylongcommandwithoutspaces", "verylongcommandwith"),
            // Command with multiple consecutive spaces
            ("hello    world    test", "hello    world"),
            // Command starting with spaces
            ("   hello world test", "   hello world"),
            // Command ending with spaces before limit
            ("hello world   test more", "hello world"),
            // Single word longer than limit
            ("supercalifragilisticexpialidocious", "supercalifragilis"),
        ];

        for (input, expected_prefix) in test_cases {
            let result = truncate_command_for_display(input, &db);

            if input.len() > 20 {
                assert!(result.ends_with("..."));
                assert!(result.len() <= 23); // 20 + "..." = 23

                // For word boundary cases, check if it starts with expected prefix
                if input.contains(' ') {
                    // Should truncate at word boundary
                    let without_ellipsis = &result[..result.len() - 3];
                    assert!(
                        expected_prefix.starts_with(without_ellipsis) || without_ellipsis.starts_with(expected_prefix)
                    );
                }
            } else {
                assert_eq!(result, input);
            }
        }
    }

    #[tokio::test]
    async fn test_integration_performance_with_large_operations() {
        let mut db = Database::new().await.unwrap();

        db.settings.set(Setting::ChatHistoryMaxLength, 50).await.unwrap();

        // Test that truncation works efficiently with many operations
        let test_command = "this is a test command that should be truncated efficiently";

        let start = std::time::Instant::now();
        for _ in 0..1000 {
            let _result = truncate_command_for_display(&test_command, &db);
        }
        let duration = start.elapsed();

        // Should complete quickly (less than 100ms for 1000 operations)
        assert!(duration.as_millis() < 100, "Truncation took too long: {:?}", duration);

        // Verify the result is still correct
        let result = truncate_command_for_display(&test_command, &db);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 53); // 50 + "..." = 53
    }

    #[tokio::test]
    async fn test_integration_database_settings_behavior() {
        // Test that settings work correctly within a single database instance
        let mut db = Database::new().await.unwrap();

        // Set a custom value and test
        db.settings.set(Setting::ChatHistoryMaxLength, 25).await.unwrap();

        let test_command = "this is a test command for settings behavior";
        let result = truncate_command_for_display(&test_command, &db);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 28); // 25 + "..." = 28
        assert!(should_display_history(&db));

        // Change to zero setting and test
        db.settings.set(Setting::ChatHistoryMaxLength, 0).await.unwrap();

        let result = truncate_command_for_display(&test_command, &db);
        assert_eq!(result, "");
        assert!(!should_display_history(&db));

        // Change back to positive value and test
        db.settings.set(Setting::ChatHistoryMaxLength, 35).await.unwrap();

        let result = truncate_command_for_display(&test_command, &db);
        assert!(result.ends_with("..."));
        assert!(result.len() <= 38); // 35 + "..." = 38
        assert!(should_display_history(&db));
    }

    #[tokio::test]
    async fn test_integration_function_consistency() {
        let mut db = Database::new().await.unwrap();

        // Test that both functions handle settings consistently across different values
        let test_values = vec![0i32, 1, 10, 25, 50, 80, 100, -5, -100];
        let test_command = "this is a test command for consistency checking";

        for &value in &test_values {
            db.settings.set(Setting::ChatHistoryMaxLength, value).await.unwrap();

            let should_display = should_display_history(&db);
            let truncated = truncate_command_for_display(&test_command, &db);

            // Consistency check: if should_display is false, truncated should be empty
            if !should_display {
                assert_eq!(truncated, "");
                assert_eq!(value, 0); // Only zero should disable display
            } else {
                // If display is enabled, truncated should not be empty for non-empty input
                assert!(!truncated.is_empty());

                // Effective value should be positive (negative values use default)
                let effective_value = if value < 0 { 80 } else { value };

                if test_command.len() > effective_value as usize {
                    assert!(truncated.ends_with("..."));
                    assert!(truncated.len() <= (effective_value as usize + 3));
                } else {
                    assert_eq!(truncated, test_command);
                }
            }
        }
    }
}
