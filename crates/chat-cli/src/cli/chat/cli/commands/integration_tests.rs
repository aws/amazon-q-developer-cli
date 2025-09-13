//! Integration tests for CLI Commands Subcommands
//!
//! This module tests the complete integration of the CommandsSubcommand
//! with the underlying CommandManager and file system operations.

use tempfile::TempDir;

use crate::os::Os;
use crate::util::command_manager::CommandManager;
use crate::util::command_manager::test_utils::*;
use crate::util::command_types::CommandScope;

/// Helper to create a test command manager with temporary directories
async fn create_test_manager() -> (CommandManager, TempDir) {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let project_commands_dir = temp_dir.path().join(".amazonq").join("commands");
    let user_commands_dir = temp_dir.path().join("home").join(".amazonq").join("commands");

    std::fs::create_dir_all(&project_commands_dir).expect("Failed to create project commands dir");
    std::fs::create_dir_all(&user_commands_dir).expect("Failed to create user commands dir");

    let manager = CommandManager::new_for_test(project_commands_dir, user_commands_dir);

    (manager, temp_dir)
}

/// Test CommandsSubcommand::Add integration
#[tokio::test]
async fn test_commands_add_integration() {
    let (mut manager, _temp_dir) = create_test_manager().await;
    let os = Os::new().await.expect("Failed to create OS instance");

    let command_name = "integration-add-test";

    // Mock editor
    unsafe {
        std::env::set_var("EDITOR", "true");
    }

    // Test add command
    let add_result = manager.add_command(command_name, &os);

    unsafe {
        std::env::remove_var("EDITOR");
    }

    assert!(add_result.is_ok(), "Add command should succeed");

    let success_message = add_result.unwrap();
    assert!(success_message.contains("✅ Command 'integration-add-test' created successfully!"));
    assert!(success_message.contains("Use '/project:integration-add-test' to execute it"));

    // Verify file was created
    let expected_file = manager.get_project_commands_dir().join(format!("{}.md", command_name));
    assert!(expected_file.exists(), "Command file should be created");

    // Verify command is cached
    assert!(
        manager.get_cache().contains_key(command_name),
        "Command should be cached"
    );

    // Verify command can be retrieved
    let retrieved_command = manager.get_command(command_name);
    assert!(retrieved_command.is_ok(), "Should be able to retrieve added command");
}

/// Test CommandsSubcommand::Show integration
#[tokio::test]
async fn test_commands_show_integration() {
    let test_fs = TestFileSystem::new().expect("Failed to create test filesystem");
    let mut manager = test_fs.create_manager();

    // Create test commands
    let test_commands = vec![
        ("show-test-1", CommandScope::Project),
        ("show-test-2", CommandScope::Project),
        ("show-test-global", CommandScope::Global),
    ];

    for (name, scope) in &test_commands {
        TestCommandBuilder::new(name)
            .with_scope(scope.clone())
            .create_in_filesystem(&test_fs)
            .expect("Failed to create test command");
    }

    // Test show all commands
    let all_commands = manager.list_commands_detailed(None);
    assert!(all_commands.is_ok(), "Should list all commands");

    let commands = all_commands.unwrap();
    assert_eq!(commands.len(), 3, "Should have 3 commands");

    // Test show project commands only
    let project_commands = manager.list_commands_detailed(Some(&CommandScope::Project));
    assert!(project_commands.is_ok(), "Should list project commands");

    let proj_commands = project_commands.unwrap();
    assert_eq!(proj_commands.len(), 2, "Should have 2 project commands");

    // Test show global commands only
    let global_commands = manager.list_commands_detailed(Some(&CommandScope::Global));
    assert!(global_commands.is_ok(), "Should list global commands");

    let glob_commands = global_commands.unwrap();
    assert_eq!(glob_commands.len(), 1, "Should have 1 global command");

    // Test show specific command
    let specific_command = manager.get_command("show-test-1");
    assert!(specific_command.is_ok(), "Should get specific command");

    let command = specific_command.unwrap();
    assert_eq!(command.name, "show-test-1", "Command name should match");
}

/// Test CommandsSubcommand::Remove integration
#[tokio::test]
async fn test_commands_remove_integration() {
    let test_fs = TestFileSystem::new().expect("Failed to create test filesystem");
    let mut manager = test_fs.create_manager();

    let command_name = "remove-test";

    // Create test command
    let command_file = TestCommandBuilder::new(command_name)
        .with_content("# Remove Test\n\nThis command will be removed.")
        .create_in_filesystem(&test_fs)
        .expect("Failed to create test command");

    // Verify command exists
    let initial_command = manager.get_command(command_name);
    assert!(initial_command.is_ok(), "Command should exist initially");

    // Remove the command file (simulating remove operation)
    std::fs::remove_file(&command_file).expect("Failed to remove command file");

    // Clear cache to force reload
    manager.clear_cache_for_test();

    // Verify command is gone
    let removed_command = manager.get_command(command_name);
    assert!(removed_command.is_err(), "Command should not exist after removal");

    // Verify it doesn't appear in listings
    let commands = manager.list_commands_detailed(None).unwrap();
    assert!(
        !commands.iter().any(|cmd| cmd.name == command_name),
        "Removed command should not appear in listings"
    );
}

/// Test CommandsSubcommand::Update integration
#[tokio::test]
async fn test_commands_update_integration() {
    let test_fs = TestFileSystem::new().expect("Failed to create test filesystem");
    let mut manager = test_fs.create_manager();

    let command_name = "update-test";
    let initial_content = "# Initial Update Test\n\nOriginal content.";
    let updated_content = "# Updated Test Command\n\nThis content has been updated.";

    // Create initial command
    let command_file = TestCommandBuilder::new(command_name)
        .with_content(initial_content)
        .create_in_filesystem(&test_fs)
        .expect("Failed to create test command");

    // Load command into cache
    let initial_command = manager.get_command(command_name);
    assert!(initial_command.is_ok(), "Should load initial command");

    let initial_cmd = initial_command.unwrap();
    assertions::assert_command_has_content(initial_cmd, "Initial Update Test");

    // Update the file (simulating editor update)
    std::fs::write(&command_file, updated_content).expect("Failed to update command file");

    // Clear cache to force reload
    manager.clear_cache_for_test();

    // Verify updated content
    let updated_command = manager.get_command(command_name);
    assert!(updated_command.is_ok(), "Should load updated command");

    let updated_cmd = updated_command.unwrap();
    assertions::assert_command_has_content(updated_cmd, "Updated Test Command");
}

/// Test error scenarios in command operations
#[tokio::test]
async fn test_commands_error_scenarios() {
    let test_fs = TestFileSystem::new().expect("Failed to create test filesystem");
    let mut manager = test_fs.create_manager();
    let os = Os::new().await.expect("Failed to create OS instance");

    // Test 1: Add command with invalid name
    unsafe {
        std::env::set_var("EDITOR", "true");
    }

    let long_name = "a".repeat(100);
    let invalid_names = vec![
        "invalid name with spaces",
        "123-starts-with-number",
        "special@characters",
        "",
        &long_name, // Too long
    ];

    for invalid_name in invalid_names {
        let result = manager.add_command(invalid_name, &os);
        assert!(result.is_err(), "Should reject invalid command name: {}", invalid_name);
    }

    unsafe {
        std::env::remove_var("EDITOR");
    }

    // Test 2: Get non-existent command
    let missing_result = manager.get_command("non-existent-command");
    assert!(missing_result.is_err(), "Should fail to get non-existent command");

    // Test 3: Duplicate command creation
    let duplicate_name = "duplicate-test";
    let duplicate_file = TestCommandBuilder::new(duplicate_name)
        .create_in_filesystem(&test_fs)
        .expect("Failed to create initial command");

    unsafe {
        std::env::set_var("EDITOR", "true");
    }

    let duplicate_result = manager.add_command(duplicate_name, &os);

    unsafe {
        std::env::remove_var("EDITOR");
    }

    assert!(duplicate_result.is_err(), "Should fail to create duplicate command");

    // Clean up
    std::fs::remove_file(&duplicate_file).expect("Failed to remove duplicate file");
}
