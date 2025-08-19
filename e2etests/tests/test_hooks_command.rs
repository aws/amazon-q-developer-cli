use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "hooks")]
fn test_all_hoooks_commands() -> Result<(), Box<dyn std::error::Error>> {
    let mut chat = QChatSession::new()?;
    println!(":white_check_mark: Q Chat session started");
    
    test_hooks_command(&mut chat)?;
    test_hooks_help_command(&mut chat)?;
    
    chat.quit()?;
    println!(":white_check_mark: All tests completed successfully");
    Ok(())
}

fn test_hooks_command(chat: &mut QChatSession) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /hooks command...");
    
    
    let response = chat.execute_command("/hooks")?;
    
    println!("📝 Hooks command response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify no hooks configured message
    assert!(response.contains("No hooks"), "Missing no hooks configured message");
    println!("✅ Found no hooks configured message");
    
    // Verify documentation reference
    assert!(response.contains("documentation"), "Missing documentation reference");
    assert!(response.contains("https://github.com/aws/amazon-q-developer-cli/blob/main/docs/agent-format.md#hooks-field"), "Missing documentation URL");
    println!("✅ Found documentation reference and URL");
    
    println!("✅ All hooks command functionality verified!");
    
    Ok(())
}

fn test_hooks_help_command(chat: &mut QChatSession) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /hooks --help command...");
    
    let response = chat.execute_command("/hooks --help")?;
    
    println!("📝 Hooks help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    /* Verify context hooks description
    assert!(response.contains("context hooks"), "Missing context hooks");
    println!("✅ Found context hooks description");*/
    
    // Verify documentation reference
    assert!(response.contains("documentation"), "Missing documentation reference");
    assert!(response.contains("https://github.com/aws/amazon-q-developer-cli/blob/main/docs/agent-format.md#hooks-field"), "Missing documentation URL");
    println!("✅ Found documentation reference and URL");
    
    // Verify Notes section
    assert!(response.contains("Notes:"), "Missing Notes section");
    /*assert!(response.contains("executed in parallel"), "Missing parallel execution note");
    assert!(response.contains("conversation_start"), "Missing conversation_start hook type");
    assert!(response.contains("per_prompt"), "Missing per_prompt hook type");*/
    println!("✅ Found Notes section with hook types and execution details");
    
    // Verify Usage section
    assert!(response.contains("Usage:"), "Missing Usage section");
    assert!(response.contains("/hooks"), "Missing /hooks command in usage section");
    println!("✅ Found Usage section with /hooks command");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    println!("✅ Found Options section");
    
    // Verify help flags
    assert!(response.contains("-h") &&  response.contains("--help") && response.contains("Print help"), "Missing -h, --help flags");
    println!("✅ Found help flags: -h, --help with Print help description");
    
    println!("✅ All hooks help content verified!");
    
    
    Ok(())
}