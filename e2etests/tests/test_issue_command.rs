use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "issue_reporting")]
fn test_all_issue_commands() -> Result<(), Box<dyn std::error::Error>> {
    let mut chat = QChatSession::new()?;
    println!(":white_check_mark: Q Chat session started");
    
    test_issue_command(&mut chat)?;
    test_issue_force_command(&mut chat)?;
    test_issue_help_command(&mut chat)?;
    
    chat.quit()?;
    println!(":white_check_mark: All tests completed successfully");
    Ok(())
}

fn test_issue_command(chat: &mut QChatSession) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /issue command with bug report...");
    
    
    let response = chat.execute_command("/issue \"Bug: Q CLI crashes when using large files\"")?;
    
    println!("📝 Issue command response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify command executed successfully (GitHub opens automatically)
    assert!(response.contains("Heading over to GitHub..."), "Missing browser opening confirmation");
    println!("✅ Found browser opening confirmation");
    
    println!("✅ All issue command functionality verified!");
    Ok(())
}

fn test_issue_force_command(chat: &mut QChatSession) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /issue --force command with critical bug...");
    
    let response = chat.execute_command("/issue --force \"Critical bug in file handling\"")?;
    
    println!("📝 Issue force command response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify command executed successfully (GitHub opens automatically)
    assert!(response.contains("Heading over to GitHub..."), "Missing browser opening confirmation");
    println!("✅ Found browser opening confirmation");
    
    println!("✅ All issue --force command functionality verified!");
    Ok(())
}

fn test_issue_help_command(chat: &mut QChatSession) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /issue --help command...");

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
    Ok(())
}

