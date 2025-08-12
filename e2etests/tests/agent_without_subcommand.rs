use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
fn agent_without_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /agent command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/agent")?;
    
    println!("📝 Agent response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify agent command structure
    assert!(response.contains("Manage agents"), "Missing 'Manage agents' description");
    assert!(response.contains("Usage:"), "Missing usage information");
    assert!(response.contains("/agent"), "Missing agent command");
    assert!(response.contains("<COMMAND>"), "Missing command placeholder");
    println!("✅ Found agent command description and usage");
    
    // Verify subcommands
    assert!(response.contains("Commands:"), "Missing Commands section");
    assert!(response.contains("list"), "Missing list subcommand");
    assert!(response.contains("create"), "Missing create subcommand");
    assert!(response.contains("schema"), "Missing schema subcommand");
    assert!(response.contains("set-default"), "Missing set-default subcommand");
    assert!(response.contains("help"), "Missing help subcommand");
    println!("✅ Verified all agent subcommands: list, create, schema, set-default, help");
    
    // Verify command descriptions
    assert!(response.contains("List all available agents"), "Missing list command description");
    assert!(response.contains("Create a new agent"), "Missing create command description");
    assert!(response.contains("Show agent config schema"), "Missing schema command description");
    assert!(response.contains("Define a default agent"), "Missing set-default command description");
    println!("✅ Verified command descriptions");
    
    // Verify options section
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-h"), "Missing short help option");
    assert!(response.contains("--help"), "Missing long help option");
    println!("✅ Found options section with help flag");
    
    println!("✅ All agent command content verified!");
    
    println!("✅ /agent command executed successfully");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}