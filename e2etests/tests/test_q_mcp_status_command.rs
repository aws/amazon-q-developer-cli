use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "mcp")]
fn test_q_mcp_status_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing q mcp status command workflow...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    // Now get list of available servers
    let list_response = chat.execute_command("q mcp list")?;
    let list_allow_response = chat.execute_command("y")?;
    
    println!("📝 List response: {} bytes", list_allow_response.len());
    println!("📝 LIST RESPONSE:");
    println!("{}", list_allow_response);
    println!("📝 END LIST RESPONSE");
    
    // Extract server names from the list
    let mut server_name: Option<String> = None;
    let lines: Vec<&str> = list_allow_response.lines().collect();
    for line in lines {
        if line.trim().starts_with("• ") {
            // Extract server name from bullet point line
            if let Some(name_part) = line.trim().strip_prefix("• ") {
                let parts: Vec<&str> = name_part.split_whitespace().collect();
                if let Some(name) = parts.first() {
                    server_name = Some(name.to_string());
                    break;
                }
            }
        }
    }
    
    if let Some(name) = server_name {
        println!("✅ Found server to test: {}", name);
        
        // Test q mcp status with specific server name
        let status_cmd = format!("q mcp status --name {}", name);
        let server_status_response = chat.execute_command(&status_cmd)?;
        let server_status_allow = chat.execute_command("y")?;
        
        println!("📝 Server status response: {} bytes", server_status_allow.len());
        println!("📝 SERVER STATUS RESPONSE:");
        println!("{}", server_status_allow);
        println!("📝 END SERVER STATUS RESPONSE");
        
        // Verify server status output
        assert!(server_status_allow.contains("Scope"), "Missing Scope field");
        assert!(server_status_allow.contains("Agent"), "Missing Agent field");
        assert!(server_status_allow.contains("Command"), "Missing Command field");
        assert!(server_status_allow.contains("Timeout"), "Missing Timeout field");
        assert!(server_status_allow.contains("Disabled"), "Missing Disabled field");
        assert!(server_status_allow.contains("Completed in"), "Missing completion indicator");
        println!("✅ Found all expected server status fields");
        
    } else {
        println!("ℹ️ No servers found to test status command");
    }
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}