use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "integration")]
fn test_hooks_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /hooks command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/hooks")?;
    
    println!("📝 Hooks command response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify no hooks configured message
    assert!(response.contains("No hooks are configured."), "Missing no hooks configured message");
    println!("✅ Found no hooks configured message");
    
    // Verify documentation reference
    assert!(response.contains("Refer to the documentation"), "Missing documentation reference");
    assert!(response.contains("https://github.com/aws/amazon-q-developer-cli/blob/main/docs/agent-format.md#hooks-field"), "Missing documentation URL");
    println!("✅ Found documentation reference and URL");
    
    // Verify hooks field reference
    assert!(response.contains("hooks-field"), "Missing hooks field reference");
    println!("✅ Found hooks field reference");
    
    println!("✅ All hooks command functionality verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}