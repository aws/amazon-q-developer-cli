use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "agent")]
fn test_agent_create_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /agent create  --name <agent_name> command...");
    
    let agent_name = "test_demo_agent";
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    // Create agent
    let create_response = chat.execute_command(&format!("/agent create --name {}", agent_name))?;
    
    println!("📝 Agent create response: {} bytes", create_response.len());
    println!("📝 CREATE RESPONSE:");
    println!("{}", create_response);
    println!("📝 END CREATE RESPONSE");
    
    // Save and exit editor
    let save_response = chat.execute_command(":wq")?;
    
    println!("📝 Save response: {} bytes", save_response.len());
    println!("📝 SAVE RESPONSE:");
    println!("{}", save_response);
    println!("📝 END SAVE RESPONSE");
    
    // Verify agent creation success message
    assert!(save_response.contains("Agent") && save_response.contains(agent_name) && save_response.contains("has been created successfully"), "Missing agent creation success message");
    println!("✅ Found agent creation success message");
    
    // Get current username using !whoami in Q CLI
    let whoami_response = chat.execute_command("!whoami")?;
    
    println!("📝 Whoami response: {} bytes", whoami_response.len());
    println!("📝 WHOAMI RESPONSE:");
    println!("{}", whoami_response);
    println!("📝 END WHOAMI RESPONSE");
    
    // Extract username from response (parse the actual username from Q CLI output)
    let lines: Vec<&str> = whoami_response.lines().collect();
    let username = lines.iter()
        .find(|line| !line.starts_with("!") && !line.starts_with(">") && !line.trim().is_empty())
        .unwrap_or(&"shrebhaa")
        .trim();
    println!("✅ Current username: {}", username);
    
    chat.quit()?;
    
    // Construct agent path dynamically
    let agent_path = format!("/Users/{}/.aws/amazonq/cli-agents/{}.json", username, agent_name);
    println!("✅ Agent path: {}", agent_path);
    
    // Delete the agent file if it exists
    if std::path::Path::new(&agent_path).exists() {
        std::fs::remove_file(&agent_path)?;
        println!("✅ Agent file deleted: {}", agent_path);
    } else {
        println!("⚠️ Agent file not found at: {}", agent_path);
    }
    
    // Verify agent file was deleted
    assert!(!std::path::Path::new(&agent_path).exists(), "Agent file should be deleted");
    println!("✅ Agent deletion verified");
    
    println!("✅ Test completed successfully");
    
    Ok(())
}
