use std::fs;
use std::path::PathBuf;

use serde_json;
use tempfile::TempDir;

// Helper function to create a temporary directory with test files
fn setup_test_environment() -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let config_dir = temp_dir.path().join("config");
    let profiles_dir = config_dir.join("profiles");

    fs::create_dir_all(&profiles_dir).expect("Failed to create profiles directory");

    // Create some test files
    let test_files_dir = temp_dir.path().join("files");
    fs::create_dir_all(&test_files_dir).expect("Failed to create test files directory");

    let file1_path = test_files_dir.join("file1.md");
    let file2_path = test_files_dir.join("file2.md");
    let file3_path = test_files_dir.join("file3.md");

    fs::write(&file1_path, "This is test file 1").expect("Failed to write test file 1");
    fs::write(&file2_path, "This is test file 2").expect("Failed to write test file 2");
    fs::write(&file3_path, "This is test file 3").expect("Failed to write test file 3");

    // Create a subdirectory with more test files
    let subdir = test_files_dir.join("subdir");
    fs::create_dir_all(&subdir).expect("Failed to create subdirectory");

    let file4_path = subdir.join("file4.md");
    fs::write(&file4_path, "This is test file 4").expect("Failed to write test file 4");

    (temp_dir, test_files_dir)
}

// Simple test to verify the test environment setup works
#[test]
fn test_setup_environment() {
    let (temp_dir, test_files_dir) = setup_test_environment();

    // Verify the test files were created
    assert!(test_files_dir.join("file1.md").exists());
    assert!(test_files_dir.join("file2.md").exists());
    assert!(test_files_dir.join("file3.md").exists());
    assert!(test_files_dir.join("subdir").join("file4.md").exists());

    // Verify the config directories were created
    assert!(temp_dir.path().join("config").exists());
    assert!(temp_dir.path().join("config").join("profiles").exists());
}

// Test that we can create and read JSON config files
#[test]
fn test_json_config_persistence() {
    let (temp_dir, _) = setup_test_environment();
    let config_dir = temp_dir.path().join("config");

    // Create a test config file
    let config_file = config_dir.join("test_config.json");
    let test_config = serde_json::json!({
        "paths": ["test/path1.md", "test/path2.md"]
    });

    fs::write(&config_file, test_config.to_string()).expect("Failed to write config file");

    // Read the config file back
    let content = fs::read_to_string(&config_file).expect("Failed to read config file");
    let parsed_config: serde_json::Value = serde_json::from_str(&content).expect("Failed to parse config");

    // Verify the content
    assert_eq!(parsed_config["paths"][0], "test/path1.md");
    assert_eq!(parsed_config["paths"][1], "test/path2.md");
}

// Test file operations that would be used by the context manager
#[test]
fn test_file_operations() {
    let (_temp_dir, test_files_dir) = setup_test_environment();

    // Read a file's content
    let file1_path = test_files_dir.join("file1.md");
    let content = fs::read_to_string(&file1_path).expect("Failed to read file");
    assert_eq!(content, "This is test file 1");

    // Create a new file
    let new_file_path = test_files_dir.join("new_file.md");
    fs::write(&new_file_path, "This is a new file").expect("Failed to write new file");
    assert!(new_file_path.exists());

    // Delete a file
    fs::remove_file(&new_file_path).expect("Failed to delete file");
    assert!(!new_file_path.exists());
}

// Test directory operations that would be used by the context manager
#[test]
fn test_directory_operations() {
    let (temp_dir, _) = setup_test_environment();

    // Create a new directory
    let new_dir_path = temp_dir.path().join("new_dir");
    fs::create_dir(&new_dir_path).expect("Failed to create directory");
    assert!(new_dir_path.exists());

    // Create a file in the new directory
    let file_in_new_dir = new_dir_path.join("file.md");
    fs::write(&file_in_new_dir, "File in new directory").expect("Failed to write file");
    assert!(file_in_new_dir.exists());

    // List files in a directory
    let entries = fs::read_dir(&new_dir_path).expect("Failed to read directory");
    let files: Vec<PathBuf> = entries.filter_map(Result::ok).map(|entry| entry.path()).collect();

    assert_eq!(files.len(), 1);
    assert_eq!(files[0], file_in_new_dir);
}

// Test path manipulation that would be used by the context manager
#[test]
fn test_path_manipulation() {
    let (_temp_dir, test_files_dir) = setup_test_environment();

    // Convert path to string
    let path_str = test_files_dir.to_string_lossy().to_string();
    assert!(path_str.contains("files"));

    // Join paths
    let joined_path = test_files_dir.join("subdir").join("file4.md");
    assert!(joined_path.ends_with("subdir/file4.md"));

    // Check if path is absolute
    assert!(test_files_dir.is_absolute());
}

// Test error handling that would be used by the context manager
#[test]
fn test_error_handling() {
    let (_temp_dir, test_files_dir) = setup_test_environment();

    // Try to read a non-existent file
    let result = fs::read_to_string(test_files_dir.join("non_existent.md"));
    assert!(result.is_err());

    // Try to create a file in a non-existent directory
    let result = fs::write(test_files_dir.join("non_existent_dir").join("file.md"), "content");
    assert!(result.is_err());
}

// Test simulating context profile operations
#[test]
fn test_simulated_profile_operations() {
    let (temp_dir, _) = setup_test_environment();
    let config_dir = temp_dir.path().join("config");
    let profiles_dir = config_dir.join("profiles");

    // Create a default profile
    let default_profile_path = profiles_dir.join("default.json");
    let default_config = serde_json::json!({
        "paths": ["path1.md"]
    });
    fs::write(&default_profile_path, default_config.to_string()).expect("Failed to write default profile");

    // Create a test profile
    let test_profile_path = profiles_dir.join("test-profile.json");
    let test_config = serde_json::json!({
        "paths": ["path2.md", "path3.md"]
    });
    fs::write(&test_profile_path, test_config.to_string()).expect("Failed to write test profile");

    // Verify profiles were created
    assert!(default_profile_path.exists());
    assert!(test_profile_path.exists());

    // Read the profiles
    let default_content = fs::read_to_string(&default_profile_path).expect("Failed to read default profile");
    let default_parsed: serde_json::Value =
        serde_json::from_str(&default_content).expect("Failed to parse default profile");

    let test_content = fs::read_to_string(&test_profile_path).expect("Failed to read test profile");
    let test_parsed: serde_json::Value = serde_json::from_str(&test_content).expect("Failed to parse test profile");

    // Verify profile contents
    assert_eq!(default_parsed["paths"].as_array().unwrap().len(), 1);
    assert_eq!(default_parsed["paths"][0], "path1.md");

    assert_eq!(test_parsed["paths"].as_array().unwrap().len(), 2);
    assert_eq!(test_parsed["paths"][0], "path2.md");
    assert_eq!(test_parsed["paths"][1], "path3.md");

    // Simulate renaming a profile
    let new_profile_path = profiles_dir.join("new-profile.json");
    fs::copy(&test_profile_path, &new_profile_path).expect("Failed to copy profile");
    fs::remove_file(&test_profile_path).expect("Failed to remove old profile");

    assert!(!test_profile_path.exists());
    assert!(new_profile_path.exists());

    // List profiles
    let profiles: Vec<String> = fs::read_dir(&profiles_dir)
        .expect("Failed to read profiles directory")
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().map_or(false, |ext| ext == "json"))
        .filter_map(|entry| entry.path().file_stem().map(|name| name.to_string_lossy().to_string()))
        .collect();

    assert_eq!(profiles.len(), 2);
    assert!(profiles.contains(&"default".to_string()));
    assert!(profiles.contains(&"new-profile".to_string()));
}

// Test simulating glob pattern expansion
#[test]
fn test_simulated_glob_expansion() {
    let (_temp_dir, test_files_dir) = setup_test_environment();

    // Create a pattern that matches all .md files
    let pattern = format!("{}/*.md", test_files_dir.to_string_lossy());

    // Manually list all .md files in the directory
    let entries = fs::read_dir(&test_files_dir).expect("Failed to read directory");
    let md_files: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().map_or(false, |ext| ext == "md"))
        .collect();

    // Verify we found the expected files
    assert_eq!(md_files.len(), 3);
    assert!(md_files.contains(&test_files_dir.join("file1.md")));
    assert!(md_files.contains(&test_files_dir.join("file2.md")));
    assert!(md_files.contains(&test_files_dir.join("file3.md")));

    // Print the pattern for debugging
    println!("Glob pattern: {}", pattern);
    println!("Found {} .md files", md_files.len());
    for file in &md_files {
        println!("  {}", file.display());
    }
}
