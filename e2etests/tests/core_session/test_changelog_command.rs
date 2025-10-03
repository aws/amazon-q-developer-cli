#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;
use regex::Regex;

#[test]
#[cfg(all(feature = "changelog", feature = "sanity"))]
fn test_changelog_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /changelog command... | Description: Tests the <code> /changelog</code> command to display version history and updates");
    
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/changelog")?;
    
    println!("📝 Changelog response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify changelog content
    assert!(response.contains("New") && response.contains("Amazon Q CLI"), "Missing changelog header");
    println!("✅ Found changelog header");
    
    // Verify version format (e.g., 1.16.2)
    let version_regex = Regex::new(r"## \d+\.\d+\.\d+").unwrap();
    assert!(version_regex.is_match(&response), "Missing version format (x.x.x)");
    println!("✅ Found valid version format");
    
    // Verify date format (e.g., 2025-09-19)
    let date_regex = Regex::new(r"\(\d{4}-\d{2}-\d{2}\)").unwrap();
    assert!(date_regex.is_match(&response), "Missing date format (YYYY-MM-DD)");
    println!("✅ Found valid date format");
    
    // Verify /changelog command reference
    assert!(response.contains("/changelog"), "Missing /changelog command reference");
    println!("✅ Found /changelog command reference");
    
    println!("✅ /changelog command test completed successfully");
    
    // Release the lock
    drop(chat);
    
    Ok(())
}

#[test]
#[cfg(all(feature = "changelog", feature = "sanity"))]
fn test_changelog_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /changelog -h command... | Description: Tests the <code> /changelog -h</code> command to display help information");
    
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/changelog -h")?;
    
    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify help content
    assert!(response.contains("Usage:") && response.contains("/changelog"), "Missing usage information");
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-h") &&  response.contains("--help"), "Missing -h, --help flags");
    println!("✅ Found all expected help content");

    println!("✅ /changelog -h command test completed successfully");
    
    // Release the lock
    drop(chat);
    
    Ok(())
}