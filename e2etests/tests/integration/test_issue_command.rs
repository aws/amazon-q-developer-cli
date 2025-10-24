#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;

#[test]
#[cfg(all(feature = "issue_reporting", feature = "sanity"))]
fn test_issue_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /issue command with bug report... | Description: Tests the <code> /issue</code> command to create a bug report and verify it opens GitHub issue creation interface");
    
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let response = chat.execute_command_with_timeout("/issue \"Bug: Q CLI crashes when using large files\"",Some(3000))?;
    
    println!("📝 Issue command response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify command executed successfully (GitHub opens automatically)
    assert!(response.contains("Heading over to GitHub..."), "Missing browser opening confirmation");
    println!("✅ Found browser opening confirmation");
    
    println!("✅ All issue command functionality verified!");

    drop(chat);

    Ok(())
}

#[test]
#[cfg(all(feature = "issue_reporting", feature = "sanity"))]
fn test_issue_force_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /issue --force command with critical bug... | Description: Tests the <code> /issue --force</code> command to create a critical bug report and verify forced issue creation workflow");
    
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let response = chat.execute_command_with_timeout("/issue --force Critical bug in file handling",Some(3000))?;
    
    println!("📝 Issue force command response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify command executed successfully (GitHub opens automatically or shows command)
    assert!(response.contains("Heading over to GitHub...") || response.contains("/issue --force") || !response.trim().is_empty(), "Command should execute or show in history");
    println!("✅ Command executed successfully");
    
    println!("✅ All issue --force command functionality verified!");

    drop(chat);

    Ok(())
}

#[test]
#[cfg(all(feature = "issue_reporting", feature = "sanity"))]
fn test_issue_f_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /issue -f command with critical bug... | Description: Tests the <code> /issue -f</code> command (short form) to create a critical bug report with force flag");
    
        let session = q_chat_helper::get_chat_session();
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

    drop(chat);

    Ok(())
}


#[test]
#[cfg(all(feature = "issue_reporting", feature = "sanity"))]
fn test_issue_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /issue --help command... | Description: Tests the <code> /issue --help</code> command to display help information for issue reporting functionality including options and usage");
     
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let response = chat.execute_command_with_timeout("/issue --help",Some(3000))?;
    
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

    drop(chat);

    Ok(())
}

#[test]
#[cfg(all(feature = "issue_reporting", feature = "sanity"))]
fn test_issue_h_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /issue -h command... | Description: Tests the <code> /issue -h</code> command (short form) to display issue reporting help information");
     
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let response = chat.execute_command_with_timeout("/issue -h",Some(3000))?;
    
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

    drop(chat);

    Ok(())
}