use std::path::{
    Path,
    PathBuf,
};
use std::process::Command;
use std::{
    env,
    fs,
};

use tempfile::TempDir;

// This test file focuses on CLI-specific integration tests for the context management feature

// Helper function to create a temporary directory with test files
fn setup_test_environment() -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let config_dir = temp_dir.path().join(".aws").join("amazonq").join("context");
    let profiles_dir = config_dir.join("profiles");

    fs::create_dir_all(&profiles_dir).expect("Failed to create profiles directory");

    // Create some test files
    let test_files_dir = temp_dir.path().join("files");
    fs::create_dir_all(&test_files_dir).expect("Failed to create test files directory");

    let file1_path = test_files_dir.join("file1.md");
    let file2_path = test_files_dir.join("file2.md");

    fs::write(&file1_path, "This is test file 1").expect("Failed to write test file 1");
    fs::write(&file2_path, "This is test file 2").expect("Failed to write test file 2");

    (temp_dir, test_files_dir)
}

// Helper function to run a CLI command and return the output
fn run_cli_command(args: Vec<&str>) -> std::process::Output {
    let mut command = Command::new("cargo");
    command.arg("run").arg("--bin").arg("q_cli");

    for arg in args {
        command.arg(arg);
    }

    command.output().expect("Failed to execute command")
}

// These tests are marked as ignored because they require the CLI binary to be built
// and they interact with the file system in ways that might not be suitable for automated testing
// Run with: cargo test --test context_management_cli_test -- --ignored

#[test]
#[ignore]
fn test_cli_profile_flag() {
    let (temp_dir, test_files_dir) = setup_test_environment();

    // Set the HOME environment variable to our temp directory
    env::set_var("HOME", temp_dir.path().to_str().unwrap());

    // Create a profile
    let output = run_cli_command(vec!["chat", "--accept-all", "/context profile --create test-profile"]);
    assert!(
        output.status.success(),
        "Failed to create profile: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Add a file to the test profile
    let file_path = test_files_dir.join("file1.md").to_string_lossy().to_string();
    let output = run_cli_command(vec![
        "chat",
        "--profile",
        "test-profile",
        "--accept-all",
        &format!("/context add --force {}", file_path),
    ]);
    assert!(
        output.status.success(),
        "Failed to add file to profile: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify the file was added by running the show command
    let output = run_cli_command(vec![
        "chat",
        "--profile",
        "test-profile",
        "--accept-all",
        "/context show",
    ]);
    assert!(
        output.status.success(),
        "Failed to show context: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output_str = String::from_utf8_lossy(&output.stdout);
    assert!(
        output_str.contains(&file_path),
        "Output does not contain file path: {}",
        output_str
    );

    // Verify the profile file was created
    let profile_path = temp_dir
        .path()
        .join(".aws")
        .join("amazonq")
        .join("context")
        .join("profiles")
        .join("test-profile.json");

    assert!(profile_path.exists(), "Profile file does not exist");

    // Read the profile file and verify it contains the file path
    let profile_content = fs::read_to_string(&profile_path).expect("Failed to read profile file");

    // Handle path separators for different platforms
    let normalized_path = file_path.replace('\\', "\\\\");
    assert!(
        profile_content.contains(&normalized_path),
        "Profile content does not contain file path.\nProfile content: {}\nExpected path: {}",
        profile_content,
        normalized_path
    );
}

#[test]
#[ignore]
fn test_cli_error_handling() {
    let (temp_dir, _) = setup_test_environment();

    // Set the HOME environment variable to our temp directory
    env::set_var("HOME", temp_dir.path().to_str().unwrap());

    // Try to switch to a non-existent profile
    let output = run_cli_command(vec!["chat", "--profile", "non-existent-profile", "--accept-all"]);

    // The command should still succeed, but there should be an error message
    assert!(output.status.success(), "Command failed unexpectedly");

    let error_str = String::from_utf8_lossy(&output.stderr);
    assert!(
        error_str.contains("profile") && error_str.contains("does not exist"),
        "Error message does not indicate profile not found: {}",
        error_str
    );

    // Try to add a non-existent file without --force
    let output = run_cli_command(vec!["chat", "--accept-all", "/context add non-existent-file.md"]);

    assert!(output.status.success(), "Command failed unexpectedly");

    let error_str = String::from_utf8_lossy(&output.stderr);
    assert!(
        error_str.contains("Invalid path") || error_str.contains("does not exist"),
        "Error message does not indicate invalid path: {}",
        error_str
    );
}

#[test]
#[ignore]
fn test_cli_context_persistence() {
    let (temp_dir, test_files_dir) = setup_test_environment();

    // Set the HOME environment variable to our temp directory
    env::set_var("HOME", temp_dir.path().to_str().unwrap());

    // Add a file to the global context
    let file_path = test_files_dir.join("file1.md").to_string_lossy().to_string();
    let output = run_cli_command(vec![
        "chat",
        "--accept-all",
        &format!("/context add --global --force {}", file_path),
    ]);
    assert!(
        output.status.success(),
        "Failed to add file to global context: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Start a new session and verify the file is still in the context
    let output = run_cli_command(vec!["chat", "--accept-all", "/context show"]);
    assert!(
        output.status.success(),
        "Failed to show context: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output_str = String::from_utf8_lossy(&output.stdout);
    assert!(
        output_str.contains(&file_path),
        "Output does not contain file path: {}",
        output_str
    );

    // Verify the global config file was created
    let global_path = temp_dir
        .path()
        .join(".aws")
        .join("amazonq")
        .join("context")
        .join("global.json");

    assert!(global_path.exists(), "Global config file does not exist");

    // Read the global config file and verify it contains the file path
    let global_content = fs::read_to_string(&global_path).expect("Failed to read global config file");

    // Handle path separators for different platforms
    let normalized_path = file_path.replace('\\', "\\\\");
    assert!(
        global_content.contains(&normalized_path),
        "Global config content does not contain file path.\nGlobal content: {}\nExpected path: {}",
        global_content,
        normalized_path
    );
}

#[test]
#[ignore]
fn test_cli_profile_commands() {
    let (temp_dir, _) = setup_test_environment();

    // Set the HOME environment variable to our temp directory
    env::set_var("HOME", temp_dir.path().to_str().unwrap());

    // Create a profile
    let output = run_cli_command(vec!["chat", "--accept-all", "/context profile --create test-profile"]);
    assert!(
        output.status.success(),
        "Failed to create profile: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // List profiles and verify the new profile is there
    let output = run_cli_command(vec!["chat", "--accept-all", "/context profile"]);
    assert!(
        output.status.success(),
        "Failed to list profiles: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output_str = String::from_utf8_lossy(&output.stdout);
    assert!(
        output_str.contains("test-profile"),
        "Output does not contain profile name: {}",
        output_str
    );

    // Switch to the new profile
    let output = run_cli_command(vec!["chat", "--accept-all", "/context switch test-profile"]);
    assert!(
        output.status.success(),
        "Failed to switch profile: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify the profile was switched
    let output = run_cli_command(vec!["chat", "--accept-all", "/context show"]);
    assert!(
        output.status.success(),
        "Failed to show context: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output_str = String::from_utf8_lossy(&output.stdout);
    assert!(
        output_str.contains("current profile: test-profile"),
        "Output does not indicate correct profile: {}",
        output_str
    );

    // Switch back to default profile
    let output = run_cli_command(vec!["chat", "--accept-all", "/context switch default"]);
    assert!(
        output.status.success(),
        "Failed to switch to default profile: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Delete the test profile
    let output = run_cli_command(vec!["chat", "--accept-all", "/context profile --delete test-profile"]);
    assert!(
        output.status.success(),
        "Failed to delete profile: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify the profile was deleted
    let profile_path = temp_dir
        .path()
        .join(".aws")
        .join("amazonq")
        .join("context")
        .join("profiles")
        .join("test-profile.json");

    assert!(!profile_path.exists(), "Profile file still exists after deletion");

    // Verify the profile is no longer listed
    let output = run_cli_command(vec!["chat", "--accept-all", "/context profile"]);
    assert!(
        output.status.success(),
        "Failed to list profiles: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output_str = String::from_utf8_lossy(&output.stdout);
    assert!(
        !output_str.contains("test-profile"),
        "Output still contains deleted profile: {}",
        output_str
    );
}

#[test]
#[ignore]
fn test_cli_force_flag() {
    let (temp_dir, _) = setup_test_environment();

    // Set the HOME environment variable to our temp directory
    env::set_var("HOME", temp_dir.path().to_str().unwrap());

    // Try to add a non-existent file with --force flag
    let output = run_cli_command(vec![
        "chat",
        "--accept-all",
        "/context add --force non-existent-file.md",
    ]);
    assert!(
        output.status.success(),
        "Failed to add non-existent file with force flag: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify the file was added to the configuration
    let output = run_cli_command(vec!["chat", "--accept-all", "/context show"]);
    assert!(
        output.status.success(),
        "Failed to show context: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let output_str = String::from_utf8_lossy(&output.stdout);
    assert!(
        output_str.contains("non-existent-file.md"),
        "Output does not contain non-existent file: {}",
        output_str
    );
}
