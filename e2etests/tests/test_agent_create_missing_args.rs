use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
fn test_agent_create_missing_args() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /agent create without required arguments...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/agent create")?;
    
    println!("📝 Agent create missing args response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify error message for missing required arguments
    assert!(response.contains("error:"), "Missing error message part 1a");
    assert!(response.contains("the following required arguments"), "Missing error message part 1b");
    assert!(response.contains("were not provided:"), "Missing error message part 2");
    assert!(response.contains("--name"), "Missing required name argument part 1");
    assert!(response.contains("<NAME>"), "Missing required name argument part 2");
    println!("✅ Found error message for missing required arguments");
    
    // Verify usage information
    assert!(response.contains("Usage:"), "Missing usage information part 1");
    assert!(response.contains("/agent create"), "Missing usage information part 2a");
    assert!(response.contains("--name <NAME>"), "Missing usage information part 2b");
    println!("✅ Found usage information");
    
    // Verify help suggestion
    assert!(response.contains("For more information"), "Missing help suggestion part 1");
    assert!(response.contains("try"), "Missing help suggestion part 2a");
    //TODO
    // assert!(response.contains("'--help"), "Missing help suggestion part 2b1");
    // assert!(response.contains("help'"), "Missing help suggestion part 2b2");
    println!("✅ Found help suggestion");
    
    // Verify options are listed
    assert!(response.contains("Options:"), "Missing options section");
    //TODO
    // assert!(response.contains("-n, --name"), "Missing name option part 1");
    assert!(response.contains("<NAME>"), "Missing name option part 2");
    assert!(response.contains("Name of the agent to be created"), "Missing name description");
    // assert!(response.contains("-d, --directory"), "Missing directory option part 1");
    assert!(response.contains("<DIRECTORY>"), "Missing directory option part 2");
    // assert!(response.contains("-f, --from"), "Missing from option part 1");
    assert!(response.contains("<FROM>"), "Missing from option part 2");
    // assert!(response.contains("-h, --help"), "Missing help option");
    println!("✅ Found all expected options");
    
    println!("✅ /agent create executed successfully with expected error for missing arguments");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}