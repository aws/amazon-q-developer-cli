use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "usage")]
fn test_all_usage_commands() -> Result<(), Box<dyn std::error::Error>> {
    let mut chat = QChatSession::new()?;
    println!(":white_check_mark: Q Chat session started");
    
    test_usage_command(&mut chat)?;
    test_usage_help_command(&mut chat)?;
    
    chat.quit()?;
    println!(":white_check_mark: All tests completed successfully");
    Ok(())
}
fn test_usage_command(chat: &mut QChatSession) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /usage command...");
    
   let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/usage")?;
    
    println!("📝 Tools response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify context window information
    assert!(response.contains("Current context window"), "Missing context window header");
    assert!(response.contains("tokens"), "Missing tokens used information");
    println!("✅ Found context window and token usage information");
    
    // Verify progress bar
    assert!(response.contains("%"), "Missing percentage display");
    println!("✅ Found progress bar with percentage");
    
    // Verify token breakdown sections
    assert!(response.contains(" Context files:"), "Missing Context files section");
    assert!(response.contains(" Tools:"), "Missing Tools section");
    assert!(response.contains(" Q responses:"), "Missing Q responses section");
    assert!(response.contains(" Your prompts:"), "Missing Your prompts section");
    println!("✅ Found all token breakdown sections");
    
    // Verify token counts and percentages format
    assert!(response.contains("tokens ("), "Missing token count format");
    assert!(response.contains("%)"), "Missing percentage format in breakdown");
    println!("✅ Verified token count and percentage format");
    
    // Verify Pro Tips section
    assert!(response.contains(" Pro Tips:"), "Missing Pro Tips section");
    println!("✅ Found Pro Tips section");
    
    // Verify specific tip commands
    assert!(response.contains("/compact"), "Missing /compact command tip");
    assert!(response.contains("/clear"), "Missing /clear command tip");
    assert!(response.contains("/context show"), "Missing /context show command tip");
    println!("✅ Found all command tips: /compact, /clear, /context show");
    
    println!("✅ All usage content verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}

fn test_usage_help_command(chat: &mut QChatSession) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /usage --help command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/usage --help")?;
    
    println!("📝 Usage help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    /* Verify description
    assert!(response.contains("context window ") && response.contains("usage"), "Missing usage command description");
    println!("✅ Found usage command description");*/
    
    // Verify Usage section
    assert!(response.contains("Usage:"), "Missing Usage section");
    assert!(response.contains("/usage"), "Missing /usage command in usage section");
    println!("✅ Found Usage section with /usage command");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    println!("✅ Found Options section");
    
    // Verify help flags
    assert!(response.contains("-h") &&  response.contains("--help") && response.contains("Print help"), "Missing -h, --help flags");
    println!("✅ Found help flags: -h, --help with description");
    
    println!("✅ All usage help content verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}