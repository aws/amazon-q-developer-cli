use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "agent")]
fn test_agent_set_default_missing_args() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /agent set-default without required arguments...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/agent set-default")?;
    
    println!("📝 Agent set-default missing args response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Collect all failures instead of stopping at first one
    let mut failures = Vec::new();
    
    if !response.contains("error") { failures.push("Missing error message"); }
    if !response.contains("the following required arguments were not provided:") { failures.push("Missing error message2"); }
    if !response.contains("--name <NAME>") { failures.push("Missing required name argument"); }
    if !response.contains("Usage:") { failures.push("Missing usage text"); }
    if !response.contains("/agent") { failures.push("Missing agent command"); }
    if !response.contains("set-default") { failures.push("Missing set-default subcommand"); }
    if !response.contains("--name") { failures.push("Missing name flag"); }
    if !response.contains("For more information") { failures.push("Missing help text"); }
    if !response.contains("--help") { failures.push("Missing help flag"); }
    if !response.contains("Options:") { failures.push("Missing options section"); }
    if !response.contains("-n") { failures.push("Missing short name flag"); }
    if !response.contains("<NAME>") { failures.push("Missing name parameter"); }
    if !response.contains("-h") { failures.push("Missing short help flag"); }
    if !response.contains("Print help") { failures.push("Missing help description"); }
    
    // Report all failures at once
    if !failures.is_empty() {
        panic!("Test failures: {}", failures.join(", "));
    }
    
    println!("✅ All expected error messages and options found");
    
    println!("✅ /agent set-default executed successfully with expected error for missing arguments");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}