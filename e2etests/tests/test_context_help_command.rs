use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
fn test_context_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /context help command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/context help")?;
    
    println!("📝 Context help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify main description
    assert!(response.contains("Context rules determine which files are included"), "Missing context rules description");
    assert!(response.contains("Amazon Q session"), "Missing Amazon Q session reference");
    assert!(response.contains("They are derived from the current active agent"), "Missing agent derivation note");
    assert!(response.contains("provide Amazon Q with additional information"), "Missing additional information note");
    assert!(response.contains("Adding relevant files helps Q generate"), "Missing file help note");
    assert!(response.contains("more accurate and helpful responses"), "Missing accuracy note");
    println!("✅ Found context rules description");
    
    // Verify Notes section
    assert!(response.contains("Notes:"), "Missing Notes section");
    assert!(response.contains("glob patterns"), "Missing glob patterns note");
    assert!(response.contains("*.py"), "Missing Python glob example");
    assert!(response.contains("src/**/*.js"), "Missing JavaScript glob example");
    assert!(response.contains("Agent rules apply only to the current agent"), "Missing agent rules note");
    assert!(response.contains("NOT preserved between chat sessions"), "Missing session preservation note");
    assert!(response.contains("edit the agent config file"), "Missing config file note");
    println!("✅ Found Notes section with all details");
    
    // Verify manage description
    assert!(response.contains("Manage context files for the chat session"), "Missing manage description");
    println!("✅ Found manage description");
    
    // Verify Usage section
    assert!(response.contains("Usage:"), "Missing Usage section");
    assert!(response.contains("/context") && response.contains("<COMMAND>"), "Missing /context command in usage");
    println!("✅ Found Usage section");
    
    // Verify Commands section
    assert!(response.contains("Commands:"), "Missing Commands section");
    assert!(response.contains("show"), "Missing show command");
    assert!(response.contains("add"), "Missing add command");
    assert!(response.contains("remove"), "Missing remove command");
    assert!(response.contains("clear"), "Missing clear command");
    assert!(response.contains("help"), "Missing help command");
    println!("✅ Found Commands section with all subcommands");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-h"), "Missing -h flag");
    assert!(response.contains("--help"), "Missing --help flag");
    println!("✅ Found Options section with help flags");
    
    println!("✅ All context help content verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}
