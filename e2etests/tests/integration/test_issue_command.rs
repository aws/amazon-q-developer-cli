#[allow(unused_imports)]
use q_cli_e2e_tests::{get_chat_session, cleanup_if_last_test};
use std::sync::atomic::AtomicUsize;

#[allow(dead_code)]
static TEST_COUNT: AtomicUsize = AtomicUsize::new(0);

// List of covered tests
#[allow(dead_code)]
const TEST_NAMES: &[&str] = &[
    "test_issue_command",
    "test_issue_force_command",
    "test_issue_f_command",
    "test_issue_help_command",
    "test_issue_h_command",
];
#[allow(dead_code)]
const TOTAL_TESTS: usize = TEST_NAMES.len();

#[test]
#[cfg(all(feature = "issue_reporting", feature = "sanity"))]
fn test_issue_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /issue command with bug report... | Description: Tests the /issue command to create a bug report and verify it opens GitHub issue creation interface");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let response = chat.execute_command("/issue \"Bug: Q CLI crashes when using large files\"")?;
    
    println!("📝 Issue command response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify command executed successfully (GitHub opens automatically)
    assert!(response.contains("Heading over to GitHub..."), "Missing browser opening confirmation");
    println!("✅ Found browser opening confirmation");
    
    println!("✅ All issue command functionality verified!");

    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "issue_reporting", feature = "sanity"))]
fn test_issue_force_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /issue --force command with critical bug... | Description: Tests the /issue --force command to create a critical bug report and verify forced issue creation workflow");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let response = chat.execute_command("/issue --force \"Critical bug in file handling\"")?;
    
    println!("📝 Issue force command response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify command executed successfully (GitHub opens automatically)
    assert!(response.contains("Heading over to GitHub..."), "Missing browser opening confirmation");
    println!("✅ Found browser opening confirmation");
    
    println!("✅ All issue --force command functionality verified!");

    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "issue_reporting", feature = "sanity"))]
fn test_issue_f_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /issue -f command with critical bug... | Description: Tests the /issue -f command (short form) to create a critical bug report with force flag");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let response = chat.execute_command("/issue -f \"Critical bug in file handling\"")?;
    
    println!("📝 Issue force command response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify command executed successfully (GitHub opens automatically)
    assert!(response.contains("Heading over to GitHub..."), "Missing browser opening confirmation");
    println!("✅ Found browser opening confirmation");
    
    println!("✅ All issue --force command functionality verified!");

    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}


#[test]
#[cfg(all(feature = "issue_reporting", feature = "sanity"))]
fn test_issue_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /issue --help command... | Description: Tests the /issue --help command to display help information for issue reporting functionality including options and usage");
     
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let response = chat.execute_command("/issue --help")?;
    
    println!("📝 Issue help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify Usage section
    assert!(response.contains("Usage:") && response.contains("/issue") && response.contains("[DESCRIPTION]") && response.contains("[OPTIONS]"), "Missing Usage section");
    println!("✅ Found usage format");
    
    // Verify Arguments section
    assert!(response.contains("Arguments:"), "Missing Arguments section");
    println!("✅ Found Arguments section");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-f")  &&  response.contains("--force"), "Missing force option");
    assert!(response.contains("-h") &&  response.contains("--help"), "Missing -h, --help flags");
    println!("✅ Found Options section with force and help flags");
    
    println!("✅ All issue help content verified!");

     // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "issue_reporting", feature = "sanity"))]
fn test_issue_h_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /issue -h command... | Description: Tests the /issue -h command (short form) to display issue reporting help information");
     
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let response = chat.execute_command("/issue -h")?;
    
    println!("📝 Issue help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify Usage section
    assert!(response.contains("Usage:") && response.contains("/issue") && response.contains("[DESCRIPTION]") && response.contains("[OPTIONS]"), "Missing Usage section");
    println!("✅ Found usage format");
    
    // Verify Arguments section
    assert!(response.contains("Arguments:"), "Missing Arguments section");
    println!("✅ Found Arguments section");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-f")  &&  response.contains("--force"), "Missing force option");
    assert!(response.contains("-h") &&  response.contains("--help"), "Missing -h, --help flags");
    println!("✅ Found Options section with force and help flags");
    
    println!("✅ All issue help content verified!");

     // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}