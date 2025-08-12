use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
fn test_agent_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /agent help...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/agent help")?;
    
    println!("📝 Agent help command response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Collect all failures instead of stopping at first one
    let mut failures = Vec::new();
    
    if !response.contains("Agents allow you to organize") { failures.push("Missing description"); }
    if !response.contains("manage different sets of context") { failures.push("Missing context description"); }
    if !response.contains("Notes") { failures.push("Missing notes section"); }
    if !response.contains("Launch q chat with a specific agent") { failures.push("Missing launch note"); }
    if !response.contains("--agent") { failures.push("Missing agent flag"); }
    if !response.contains("Construct an agent under") { failures.push("Missing construct note"); }
    if !response.contains("~/.aws/amazonq/cli-agents/") { failures.push("Missing global path"); }
    if !response.contains("cwd/.aws/amazonq/cli-agents") { failures.push("Missing workspace path"); }
    if !response.contains("Manage agents") { failures.push("Missing manage section"); }
    if !response.contains("Usage:") { failures.push("Missing usage label"); }
    if !response.contains("/agent") { failures.push("Missing agent command"); }
    if !response.contains("<COMMAND>") { failures.push("Missing command parameter"); }
    if !response.contains("Commands:") { failures.push("Missing commands section"); }
    if !response.contains("list") { failures.push("Missing list command"); }
    if !response.contains("create") { failures.push("Missing create command"); }
    if !response.contains("schema") { failures.push("Missing schema command"); }
    if !response.contains("set-default") { failures.push("Missing set-default command"); }
    if !response.contains("help") { failures.push("Missing help command"); }
    if !response.contains("Options:") { failures.push("Missing options section"); }
    if !response.contains("-h") { failures.push("Missing short help flag"); }
    if !response.contains("--help") { failures.push("Missing long help flag"); }
    
    // Report all failures at once
    if !failures.is_empty() {
        panic!("Test failures: {}", failures.join(", "));
    }
    
    println!("✅ All expected help content found");
    
    println!("✅ /agent help executed successfully");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}