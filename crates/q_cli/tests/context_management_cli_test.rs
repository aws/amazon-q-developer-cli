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

// This test file focuses on CLI-specific integration tests
// Note: These tests are marked as ignored by default since they require the CLI binary
// Run with: cargo test --test context_management_cli_test -- --ignored

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

#[test]
#[ignore]
fn test_cli_profile_flag() {
    let (temp_dir, test_files_dir) = setup_test_environment();

    // Set the HOME environment variable to our temp directory
    // This ensures the CLI will use our test directory for configuration
    env::set_var("HOME", temp_dir.path().to_str().unwrap());

    // Create a profile using the CLI
    let output = Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("q")
        .arg("chat")
        .arg("--accept-all")
        .arg("/context profile --create test-profile")
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());

    // Add a file to the test profile
    let file_path = test_files_dir.join("file1.md").to_string_lossy().to_string();
    let output = Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("q")
        .arg("chat")
        .arg("--profile")
        .arg("test-profile")
        .arg("--accept-all")
        .arg(format!("/context add --force {}", file_path))
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());

    // Verify the file was added by running the show command
    let output = Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("q")
        .arg("chat")
        .arg("--profile")
        .arg("test-profile")
        .arg("--accept-all")
        .arg("/context show")
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());

    let output_str = String::from_utf8_lossy(&output.stdout);
    assert!(output_str.contains(&file_path));

    // Verify the profile file was created
    let profile_path = temp_dir
        .path()
        .join(".aws")
        .join("amazonq")
        .join("context")
        .join("profiles")
        .join("test-profile.json");

    assert!(profile_path.exists());

    // Read the profile file and verify it contains the file path
    let profile_content = fs::read_to_string(profile_path).expect("Failed to read profile file");
    assert!(profile_content.contains(&file_path.replace('\\', "\\\\")));
}

#[test]
#[ignore]
fn test_cli_error_handling() {
    let (temp_dir, _) = setup_test_environment();

    // Set the HOME environment variable to our temp directory
    env::set_var("HOME", temp_dir.path().to_str().unwrap());

    // Try to switch to a non-existent profile
    let output = Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("q")
        .arg("chat")
        .arg("--profile")
        .arg("non-existent-profile")
        .arg("--accept-all")
        .arg("exit")
        .output()
        .expect("Failed to execute command");

    // The command should still succeed, but there should be an error message
    assert!(output.status.success());

    let error_str = String::from_utf8_lossy(&output.stderr);
    assert!(error_str.contains("profile") && error_str.contains("does not exist"));

    // Try to add a non-existent file without --force
    let output = Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("q")
        .arg("chat")
        .arg("--accept-all")
        .arg("/context add non-existent-file.md")
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());

    let error_str = String::from_utf8_lossy(&output.stderr);
    assert!(error_str.contains("Invalid path") || error_str.contains("does not exist"));
}

#[test]
#[ignore]
fn test_cli_context_persistence() {
    let (temp_dir, test_files_dir) = setup_test_environment();

    // Set the HOME environment variable to our temp directory
    env::set_var("HOME", temp_dir.path().to_str().unwrap());

    // Add a file to the global context
    let file_path = test_files_dir.join("file1.md").to_string_lossy().to_string();
    let output = Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("q")
        .arg("chat")
        .arg("--accept-all")
        .arg(format!("/context add --global --force {}", file_path))
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());

    // Start a new session and verify the file is still in the context
    let output = Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("q")
        .arg("chat")
        .arg("--accept-all")
        .arg("/context show")
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());

    let output_str = String::from_utf8_lossy(&output.stdout);
    assert!(output_str.contains(&file_path));

    // Verify the global config file was created
    let global_path = temp_dir
        .path()
        .join(".aws")
        .join("amazonq")
        .join("context")
        .join("global.json");

    assert!(global_path.exists());

    // Read the global config file and verify it contains the file path
    let global_content = fs::read_to_string(global_path).expect("Failed to read global config file");
    assert!(global_content.contains(&file_path.replace('\\', "\\\\")));
}

#[test]
#[ignore]
fn test_cli_profile_commands() {
    let (temp_dir, _) = setup_test_environment();

    // Set the HOME environment variable to our temp directory
    env::set_var("HOME", temp_dir.path().to_str().unwrap());

    // Create a profile
    let output = Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("q")
        .arg("chat")
        .arg("--accept-all")
        .arg("/context profile --create test-profile")
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());

    // List profiles and verify the new profile is there
    let output = Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("q")
        .arg("chat")
        .arg("--accept-all")
        .arg("/context profile")
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());

    let output_str = String::from_utf8_lossy(&output.stdout);
    assert!(output_str.contains("test-profile"));

    // Switch to the new profile
    let output = Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("q")
        .arg("chat")
        .arg("--accept-all")
        .arg("/context switch test-profile")
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());

    // Verify the profile was switched
    let output = Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("q")
        .arg("chat")
        .arg("--accept-all")
        .arg("/context show")
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());

    let output_str = String::from_utf8_lossy(&output.stdout);
    assert!(output_str.contains("current profile: test-profile"));

    // Delete the profile
    let output = Command::new("cargo")
        .arg("run")
        .arg("--bin")
        .arg("q")
        .arg("chat")
        .arg("--accept-all")
        .arg("/context switch default")
        .arg("/context profile --delete test-profile")
        .output()
        .expect("Failed to execute command");

    assert!(output.status.success());

    // Verify the profile was deleted
    let profile_path = temp_dir
        .path()
        .join(".aws")
        .join("amazonq")
        .join("context")
        .join("profiles")
        .join("test-profile.json");

    assert!(!profile_path.exists());
}
