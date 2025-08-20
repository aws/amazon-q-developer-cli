use q_cli_e2e_tests::{get_chat_session, cleanup_if_last_test};
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_COUNT: AtomicUsize = AtomicUsize::new(0);

// List of covered tests
const TEST_NAMES: &[&str] = &[
    "test_context_show_command",
    "test_context_help_command",
    "test_context_without_subcommand",
    "test_context_invalid_command",
    "test_add_non_existing_file_context",
    "test_context_remove_command_of_non_existent_file",
    "test_add_remove_file_context",
    "test_add_glob_pattern_file_context",
    "test_clear_context_command",
    "test_add_remove_multiple_file_context"
];
const TOTAL_TESTS: usize = TEST_NAMES.len();

#[test]
#[cfg(feature = "context")]
fn test_context_show_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /context show command...");

    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("/context show")?;
    
    println!("📝 Context show response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify context show output contains expected sections
    assert!(response.contains("👤 Agent"), "Missing Agent section with emoji");
    println!("✅ Found Agent section with emoji");
    
    // Verify agent configuration details
    assert!(response.contains("q_cli_default"), "Missing q_cli_default in agent config");
    assert!(response.contains("💬 Session"), "Missing session section with emoji");
    println!("✅ Found all expected agent configuration files");
    
    println!("✅ All context show content verified!");

    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(feature = "context")]
fn test_context_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /context help command...");

    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("/context help")?;
    
    println!("📝 Context help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify Usage section
    assert!(response.contains("Usage:"), "Missing Usage section");
    assert!(response.contains("/context") && response.contains("<COMMAND>"), "Missing /context command in usage");
    println!("✅ Found Usage section");
    
    // Verify Commands section
    assert!(response.contains("Commands:"), "Missing Commands section");
    assert!(response.contains("show"), "Missing show command");
    assert!(response.contains("add"), "Missing add command");
    assert!(response.contains("remove"), "Missing remove command");
    assert!(response.contains("clear"), "Missing clear command");
    assert!(response.contains("help"), "Missing help command");
    println!("✅ Found Commands section with all subcommands");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-h"), "Missing -h flag");
    assert!(response.contains("--help"), "Missing --help flag");
    println!("✅ Found Options section with help flags");
    
    println!("✅ All context help content verified!");

    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(feature = "context")]
fn test_context_without_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /context without sub command...");

    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("/context")?;
    
    println!("📝 Context response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    assert!(response.contains("Usage:"), "Missing Usage section");
    assert!(response.contains("/context") && response.contains("<COMMAND>"), "Missing /context command in usage");
    println!("✅ Found Usage section with /context command");
    
    assert!(response.contains("Commands:"), "Missing Commands section");
    assert!(response.contains("show"), "Missing show command");
    assert!(response.contains("add"), "Missing add command");
    assert!(response.contains("remove"), "Missing remove command");
    assert!(response.contains("clear"), "Missing clear command");
    assert!(response.contains("help"), "Missing help command");
    println!("✅ Found Commands section with all subcommands");
    
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-h"), "Missing -h flag");
    assert!(response.contains("--help"), "Missing --help flag");
    assert!(response.contains("Print help"), "Missing help description");
    println!("✅ Found Options section with -h, --help flags");
    
    println!("✅ All context help content verified!");

    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(feature = "context")]
fn test_context_invalid_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /context invalid command...");

    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("/context test")?;
    
    println!("📝 Context invalid response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify error message for invalid subcommand
    assert!(response.contains("error:") && response.contains("unrecognized subcommand") && response.contains("test"), "Missing 'unrecognized subcommand' error message");
    println!("✅ Found expected error message for invalid subcommand");
    
    assert!(response.contains("Usage:"), "Missing Usage section");
    assert!(response.contains("/context") && response.contains("<COMMAND>"), "Missing /context command in usage");
    println!("✅ Found Usage section with /context command");
    
    assert!(response.contains("Commands:"), "Missing Commands section");
    assert!(response.contains("show"), "Missing show command");
    assert!(response.contains("add"), "Missing add command");
    assert!(response.contains("remove"), "Missing remove command");
    assert!(response.contains("clear"), "Missing clear command");
    assert!(response.contains("help"), "Missing help command");
    println!("✅ Found Commands section with all subcommands");
    
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-h"), "Missing -h flag");
    assert!(response.contains("--help"), "Missing --help flag");
    assert!(response.contains("Print help"), "Missing help description");
    println!("✅ Found Options section with -h, --help flags");
    
    println!("✅ All context invalid command content verified!");

    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(feature = "context")]
fn test_add_non_existing_file_context() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /context add non-existing file command...");

    let non_existing_file_path = "/tmp/non_existing_file.py";

    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    // Try to add non-existing file to context
    let add_response = chat.execute_command(&format!("/context add {}", non_existing_file_path))?;
    
    println!("📝 Context add response: {} bytes", add_response.len());
    println!("📝 ADD RESPONSE:");
    println!("{}", add_response);
    println!("📝 END ADD RESPONSE");
    
    // Verify error message for non-existing file
    assert!(add_response.contains("Error:") && add_response.contains("Invalid path") && add_response.contains("does not exist"), "Missing error message for non-existing file");
    assert!(add_response.contains("Use --force to add anyway"), "Missing --force suggestion in error message");
    println!("✅ Found expected error message for non-existing file with --force suggestion");

    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(feature = "context")]
fn test_context_remove_command_of_non_existent_file() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /context remove non existing file command...");

    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("/context remove non_existent_file.txt")?;
    
    println!("📝 Context remove response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify error message for non-existent file
    assert!(response.contains("Error:"), "Missing error message for non-existent file");
    println!("✅ Found expected error message for non-existent file removal");

    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(feature = "context")]
fn test_add_remove_file_context() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /context add <filename> command and /context remove <filename> command...");

    let test_file_path = "/tmp/test_context_file_.py";
    // Create a test file
    std::fs::write(test_file_path, "# Test file for context\nprint('Hello from test file')")?;
    println!("✅ Created test file at {}", test_file_path);

    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    // Add file to context
    let add_response = chat.execute_command(&format!("/context add {}", test_file_path))?;
    
    println!("📝 Context add response: {} bytes", add_response.len());
    println!("📝 ADD RESPONSE:");
    println!("{}", add_response);
    println!("📝 END ADD RESPONSE");
    
    // Verify file was added successfully - be flexible with the exact message format
    assert!(add_response.contains("Added") && (add_response.contains("1 path(s) to context") || add_response.contains("1 path to context") || add_response.contains("1 file to context")), "Missing success message for adding file");
    println!("✅ File added to context successfully");
    
    // Execute /context show to confirm file is present
    let show_response = chat.execute_command("/context show")?;
    
    println!("📝 Context show response: {} bytes", show_response.len());
    println!("📝 SHOW RESPONSE:");
    println!("{}", show_response);
    println!("📝 END SHOW RESPONSE");
    
    // Verify file is present in context
    assert!(show_response.contains(test_file_path), "File not found in context show output");
    assert!(show_response.contains("💬 Session (temporary):"), "Missing Session section");
    println!("✅ File confirmed present in context");
    
    // Remove file from context
    let remove_response = chat.execute_command(&format!("/context remove {}", test_file_path))?;
    
    println!("📝 Context remove response: {} bytes", remove_response.len());
    println!("📝 REMOVE RESPONSE:");
    println!("{}", remove_response);
    println!("📝 END REMOVE RESPONSE");
    
    // Verify file was removed successfully - be flexible with the exact message format
    assert!(remove_response.contains("Removed") && (remove_response.contains("1 path(s) from context") || remove_response.contains("1 path from context") || remove_response.contains("1 file from context")), "Missing success message for removing file");
    println!("✅ File removed from context successfully");
    
    // Execute /context show to confirm file is gone
    let final_show_response = chat.execute_command("/context show")?;
    
    println!("📝 Final context show response: {} bytes", final_show_response.len());
    println!("📝 FINAL SHOW RESPONSE:");
    println!("{}", final_show_response);
    println!("📝 END FINAL SHOW RESPONSE");
    
    // Verify file is no longer in context
    assert!(!final_show_response.contains(test_file_path), "File still found in context after removal");
    println!("✅ File confirmed removed from context");

    // Release the lock before cleanup
    drop(chat);

    // Clean up test file
    let _ = std::fs::remove_file(test_file_path);
    println!("✅ Cleaned up test file");
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(feature = "context")]
fn test_add_glob_pattern_file_context()-> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /context add *.py glob pattern command...");

    let test_file1_path = "/tmp/test_context_file1.py";
    let test_file2_path = "/tmp/test_context_file2.py";
    let test_file3_path = "/tmp/test_context_file.js"; // Non-matching file
    let glob_pattern = "/tmp/*.py";
    
    // Create test files
    std::fs::write(test_file1_path, "# Test Python file 1 for context\nprint('Hello from Python file 1')")?;
    std::fs::write(test_file2_path, "# Test Python file 2 for context\nprint('Hello from Python file 2')")?;
    std::fs::write(test_file3_path, "// Test JavaScript file\nconsole.log('Hello from JS file');")?;
    println!("✅ Created test files at {}, {}, {}", test_file1_path, test_file2_path, test_file3_path);

    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    // Add glob pattern to context
    let add_response = chat.execute_command(&format!("/context add {}", glob_pattern))?;
    
    println!("📝 Context add response: {} bytes", add_response.len());
    println!("📝 ADD RESPONSE:");
    println!("{}", add_response);
    println!("📝 END ADD RESPONSE");
    
    // Verify glob pattern was added successfully - be flexible with the exact message format
    assert!(add_response.contains("Added") && (add_response.contains("1 path(s) to context") || add_response.contains("1 path to context") || add_response.contains("1 pattern to context")), "Missing success message for adding glob pattern");
    println!("✅ Glob pattern added to context successfully");
    
    // Execute /context show to confirm pattern matches files
    let show_response = chat.execute_command("/context show")?;
    
    println!("📝 Context show response: {} bytes", show_response.len());
    println!("📝 SHOW RESPONSE:");
    println!("{}", show_response);
    println!("📝 END SHOW RESPONSE");
    
    // Verify glob pattern is present and matches files
    assert!(show_response.contains(glob_pattern), "Glob pattern not found in context show output");
    assert!(show_response.contains("match"), "Missing match indicator for glob pattern");
    assert!(show_response.contains("💬 Session (temporary):"), "Missing Session section");
    println!("✅ Glob pattern confirmed present in context with matches");

    // Remove glob pattern from context
    let remove_response = chat.execute_command(&format!("/context remove {}", glob_pattern))?;
    
    println!("📝 Context remove response: {} bytes", remove_response.len());
    println!("📝 REMOVE RESPONSE:");
    println!("{}", remove_response);
    println!("📝 END REMOVE RESPONSE");
    
    // Verify glob pattern was removed successfully - be flexible with the exact message format
    assert!(remove_response.contains("Removed") && (remove_response.contains("1 path(s) from context") || remove_response.contains("1 path from context") || remove_response.contains("1 pattern from context")), "Missing success message for removing glob pattern");
    println!("✅ Glob pattern removed from context successfully");
    
    // Execute /context show to confirm glob pattern is gone
    let final_show_response = chat.execute_command("/context show")?;
    
    println!("📝 Final context show response: {} bytes", final_show_response.len());
    println!("📝 FINAL SHOW RESPONSE:");
    println!("{}", final_show_response);
    println!("📝 END FINAL SHOW RESPONSE");
    
    // Verify glob pattern is no longer in context
    assert!(!final_show_response.contains(glob_pattern), "Glob pattern still found in context after removal");
    println!("✅ Glob pattern confirmed removed from context");

    // Release the lock before cleanup
    drop(chat);

    // Clean up test file
    let _ = std::fs::remove_file(test_file1_path);
    let _ = std::fs::remove_file(test_file2_path);
    let _ = std::fs::remove_file(test_file3_path);
    println!("✅ Cleaned up test file");
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(feature = "context")]
fn test_clear_context_command()-> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /context clear command...");

    let test_file_path = "/tmp/test_context_file.py";
    
    // Create test files
    std::fs::write(test_file_path, "# Test Python file 1 for context\nprint('Hello from Python file 1')")?;
    println!("✅ Created test files at {}", test_file_path);

    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    // Add multiple files to context
    let add_response = chat.execute_command(&format!("/context add {}", test_file_path))?;
    
    println!("📝 Context add response: {} bytes", add_response.len());
    println!("📝 ADD RESPONSE:");
    println!("{}", add_response);
    println!("📝 END ADD RESPONSE");
    
    // Verify files were added successfully - be flexible with the exact message format
    assert!(add_response.contains("Added") && (add_response.contains("1 path(s) to context") || add_response.contains("2 paths to context") || add_response.contains("2 files to context")), "Missing success message for adding files");
    println!("✅ Files added to context successfully");
    
    // Execute /context show to confirm files are present
    let show_response = chat.execute_command("/context show")?;
    
    println!("📝 Context show response: {} bytes", show_response.len());
    println!("📝 SHOW RESPONSE:");
    println!("{}", show_response);
    println!("📝 END SHOW RESPONSE");
    
    // Verify files are present in context
    assert!(show_response.contains(test_file_path), "Python file not found in context show output");
    println!("✅ Files confirmed present in context");
    
    // Execute /context clear to remove all files
    let clear_response = chat.execute_command("/context clear")?;
    
    println!("📝 Context clear response: {} bytes", clear_response.len());
    println!("📝 CLEAR RESPONSE:");
    println!("{}", clear_response);
    println!("📝 END CLEAR RESPONSE");
    
    // Verify context was cleared successfully
    assert!(clear_response.contains("Cleared context"), "Missing success message for clearing context");
    println!("✅ Context cleared successfully");
    
    // Execute /context show to confirm no files remain
    let final_show_response = chat.execute_command("/context show")?;
    
    println!("📝 Final context show response: {} bytes", final_show_response.len());
    println!("📝 FINAL SHOW RESPONSE:");
    println!("{}", final_show_response);
    println!("📝 END FINAL SHOW RESPONSE");
    
    // Verify no files remain in context
    assert!(!final_show_response.contains(test_file_path), "Python file still found in context after clear");
    assert!(final_show_response.contains("👤 Agent (q_cli_default):"), "Missing Agent section");
    assert!(final_show_response.contains("💬 Session (temporary):"), "Missing Session section");
    assert!(final_show_response.contains("<none>"), "Missing <none> indicator for cleared context");
    println!("✅ All files confirmed removed from context and <none> sections present");

    // Release the lock before cleanup
    drop(chat);

    // Clean up test file
    let _ = std::fs::remove_file(test_file_path);
    println!("✅ Cleaned up test file");
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(feature = "context")]
fn test_add_remove_multiple_file_context()-> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /context add <filename1> <filename2> <filename3> command and /context remove <filename1> <filename2> <filename3>...");
    
    let test_file1_path = "/tmp/test_context_file1.py";
    let test_file2_path = "/tmp/test_context_file2.py";
    let test_file3_path = "/tmp/test_context_file.js";
    
    // Create test files
    std::fs::write(test_file1_path, "# Test Python file 1 for context\nprint('Hello from Python file 1')")?;
    std::fs::write(test_file2_path, "# Test Python file 2 for context\nprint('Hello from Python file 2')")?;
    std::fs::write(test_file3_path, "// Test JavaScript file\nconsole.log('Hello from JS file');")?;
    println!("✅ Created test files at {}, {}, {}", test_file1_path, test_file2_path, test_file3_path);

    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    // Add multiple files to context in one command
    let add_response = chat.execute_command(&format!("/context add {} {} {}", test_file1_path, test_file2_path, test_file3_path))?;
    
    println!("📝 Context add response: {} bytes", add_response.len());
    println!("📝 ADD RESPONSE:");
    println!("{}", add_response);
    println!("📝 END ADD RESPONSE");
    
    // Verify files were added successfully - be flexible with the exact message format
    assert!(add_response.contains("Added") && (add_response.contains("3 path(s) to context") || add_response.contains("3 paths to context") || add_response.contains("3 files to context")), "Missing success message for adding multiple files");
    println!("✅ Multiple files added to context successfully");
    
    // Execute /context show to confirm files are present
    let show_response = chat.execute_command("/context show")?;
    
    println!("📝 Context show response: {} bytes", show_response.len());
    println!("📝 SHOW RESPONSE:");
    println!("{}", show_response);
    println!("📝 END SHOW RESPONSE");
    
    // Verify all files are present in context
    assert!(show_response.contains(test_file1_path), "Python file not found in context show output");
    assert!(show_response.contains(test_file2_path), "JavaScript file not found in context show output");
    assert!(show_response.contains(test_file3_path), "Text file not found in context show output");
    assert!(show_response.contains("💬 Session (temporary):"), "Missing Session section");
    println!("✅ All files confirmed present in context");

    // Remove multiple files from context
    let remove_response = chat.execute_command(&format!("/context remove {} {} {}", test_file1_path, test_file2_path, test_file3_path))?;
    
    println!("📝 Context remove response: {} bytes", remove_response.len());
    println!("📝 REMOVE RESPONSE:");
    println!("{}", remove_response);
    println!("📝 END REMOVE RESPONSE");
    
    // Verify files were removed successfully - be flexible with the exact message format
    assert!(remove_response.contains("Removed") && (remove_response.contains("3 path(s) from context") || remove_response.contains("3 paths from context") || remove_response.contains("3 files from context")), "Missing success message for removing multiple files");
    println!("✅ Multiple files removed from context successfully");
    
    // Execute /context show to confirm files are gone
    let final_show_response = chat.execute_command("/context show")?;
    
    println!("📝 Final context show response: {} bytes", final_show_response.len());
    println!("📝 FINAL SHOW RESPONSE:");
    println!("{}", final_show_response);
    println!("📝 END FINAL SHOW RESPONSE");
    
    // Verify files are no longer in context
    assert!(!final_show_response.contains(test_file1_path), "Python file still found in context after removal");
    assert!(!final_show_response.contains(test_file2_path), "JavaScript file still found in context after removal");
    assert!(!final_show_response.contains(test_file3_path), "Text file still found in context after removal");
    println!("✅ All files confirmed removed from context");

    // Release the lock before cleanup
    drop(chat);

    // Clean up test file
    let _ = std::fs::remove_file(test_file1_path);
    let _ = std::fs::remove_file(test_file2_path);
    let _ = std::fs::remove_file(test_file3_path);
    println!("✅ Cleaned up test file");
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

