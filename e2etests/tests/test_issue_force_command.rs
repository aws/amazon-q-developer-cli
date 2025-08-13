use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "issue_reporting")]
fn test_issue_force_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /issue --force command with critical bug...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/issue --force \"Critical bug in file handling\"")?;
    
    println!("📝 Issue force command response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify command executed successfully (GitHub opens automatically)
    assert!(response.contains("Heading over to GitHub..."), "Missing browser opening confirmation");
    println!("✅ Found browser opening confirmation");
    
    println!("✅ All issue --force command functionality verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}