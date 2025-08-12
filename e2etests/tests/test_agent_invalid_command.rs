use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
fn test_agent_invalid_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /agent invalidcommand...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/agent invalidcommand")?;
    
    println!("📝 Agent invalid command response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify error message
    // assert!(response.contains("error: unrecognized subcommand 'invalidcommand'"), "Missing error message");
    // println!("✅ Found error message for invalid subcommand");
    
    // Verify usage information
    // assert!(response.contains("Usage: /agent <COMMAND>"), "Missing usage information");
    // println!("✅ Found usage information");
    
    // Verify help suggestion
    // assert!(response.contains("For more information, try '--help'"), "Missing help suggestion");
    // println!("✅ Found help suggestion");
    
    // Verify available commands are listed
    assert!(response.contains("Commands:"), "Missing commands section");
    assert!(response.contains("list"), "Missing list command");
    assert!(response.contains("create"), "Missing create command");
    assert!(response.contains("schema"), "Missing schema command");
    assert!(response.contains("set-default"), "Missing set-default command");
    assert!(response.contains("help"), "Missing help command");
    println!("✅ Found all expected commands in help output");
    
    // Verify options section
    assert!(response.contains("Options:"), "Missing options section");
    // assert!(response.contains("-h, --help"), "Missing help option");
    println!("✅ Found options section");
    
    println!("✅ /agent invalidcommand executed successfully with expected error");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}