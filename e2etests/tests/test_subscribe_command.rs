use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "subscribe")]
fn test_all_subscribe_commands() -> Result<(), Box<dyn std::error::Error>> {
    let mut chat = QChatSession::new()?;
    println!(":white_check_mark: Q Chat session started");
    
    test_subscribe_command(&mut chat)?;
    test_subscribe_help_command(&mut chat)?;
    
    chat.quit()?;
    println!(":white_check_mark: All tests completed successfully");
    Ok(())
}
fn test_subscribe_command(chat: &mut QChatSession) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /subscribe command...");

    let response = chat.execute_command("/subscribe")?;
    
    println!("📝 Subscribe response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify subscription management message
    assert!(response.contains("Q Developer Pro subscription") && response.contains("IAM Identity Center"), "Missing subscription management message");
    println!("✅ Found subscription management message");
    
    println!("✅ All subscribe content verified!");

    
    Ok(())
}

fn test_subscribe_help_command(chat: &mut QChatSession) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /subscribe --help command...");

    let response = chat.execute_command("/subscribe --help")?;
    
    println!("📝 Subscribe help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify description
    assert!(response.contains("Q Developer Pro subscription"), "Missing subscription description");
    println!("✅ Found subscription description");
    
    // Verify Usage section
    assert!(response.contains("Usage:"), "Missing Usage section");
    assert!(response.contains("/subscribe"), "Missing /subscribe command in usage section");
    assert!(response.contains("[OPTIONS]"), "Missing [OPTIONS] in usage section");
    println!("✅ Found Usage section with /subscribe [OPTIONS]");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    println!("✅ Found Options section");
    
    // Verify manage option
    assert!(response.contains("--manage"), "Missing --manage option");
    println!("✅ Found --manage option");
    
    // Verify help flags
    assert!(response.contains("-h") &&  response.contains("--help") , "Missing -h, --help flags");
    println!("✅ Found help flags: -h, --help with Print help description");
    
    println!("✅ All subscribe help content verified!");
    
    
    Ok(())
}
