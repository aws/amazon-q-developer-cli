use q_cli_e2e_tests::{get_chat_session, cleanup_if_last_test};
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_COUNT: AtomicUsize = AtomicUsize::new(0);

// List of covered tests
const TEST_NAMES: &[&str] = &[
    "test_issue_command",
    "test_issue_force_command",
    "test_issue_help_command",
];
const TOTAL_TESTS: usize = TEST_NAMES.len();

#[test]
#[cfg(feature = "issue_reporting")]
fn test_issue_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /issue command with bug report...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

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
#[cfg(feature = "issue_reporting")]
fn test_issue_force_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /issue --force command with critical bug...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

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
#[cfg(feature = "issue_reporting")]
fn test_issue_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /issue --help command...");
     
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

    let response = chat.execute_command("/issue --help")?;
    
    println!("📝 Issue help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    /* Verify description
    assert!(response.contains("issue") && response.contains("feature request"), "Missing issue description");
    println!("✅ Found issue description");*/
    
    // Verify Usage section
    //assert!(response.contains("Usage: /issue [OPTIONS] [DESCRIPTION]..."), "Missing usage format");
    assert!(response.contains("Usage:") && response.contains("/issue") && response.contains("[DESCRIPTION]") && response.contains("[OPTIONS]"), "Missing Usage section");
    println!("✅ Found usage format");
    
    // Verify Arguments section
    assert!(response.contains("Arguments:"), "Missing Arguments section");
    assert!(response.contains("[DESCRIPTION]"), "Missing DESCRIPTION argument");
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

