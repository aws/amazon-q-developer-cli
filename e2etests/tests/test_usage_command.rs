use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "session_mgmt")]
fn test_usage_command() -> Result<(), Box<dyn std::error::Error>> {
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
    assert!(response.contains("tokens used"), "Missing tokens used information");
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
    
    // Verify tip descriptions
    assert!(response.contains("replace the conversation history with its summary"), "Missing /compact description");
    assert!(response.contains("erase the entire chat history"), "Missing /clear description");
    assert!(response.contains("see tokens per context file"), "Missing /context show description");
    println!("✅ Verified all tip descriptions");
    
    println!("✅ All usage content verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}