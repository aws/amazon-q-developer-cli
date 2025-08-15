use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "integration")]
fn test_issue_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /issue --help command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/issue --help")?;
    
    println!("📝 Issue help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify description
    assert!(response.contains("Create a new Github issue or make a feature request"), "Missing issue description");
    println!("✅ Found issue description");
    
    // Verify Usage section
    //assert!(response.contains("Usage: /issue [OPTIONS] [DESCRIPTION]..."), "Missing usage format");
    assert!(response.contains("Usage:") && response.contains("/issue") && response.contains("[DESCRIPTION]") && response.contains("[OPTIONS]"), "Missing Usage section");
    println!("✅ Found usage format");
    
    // Verify Arguments section
    assert!(response.contains("Arguments:"), "Missing Arguments section");
    assert!(response.contains("[DESCRIPTION]..."), "Missing DESCRIPTION argument");
    assert!(response.contains("Issue description"), "Missing issue description text");
    println!("✅ Found Arguments section");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-f")  &&  response.contains("--force"), "Missing force option");
    assert!(response.contains("Force issue creation"), "Missing force description");
    assert!(response.contains("-h") &&  response.contains("--help"), "Missing -h, --help flags");
    assert!(response.contains("Print help"), "Missing help description");
    println!("✅ Found Options section with force and help flags");
    
    println!("✅ All issue help content verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}