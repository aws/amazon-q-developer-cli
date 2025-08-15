use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "tools")]
fn test_tools_trust_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /tools trust command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    // First get list of tools to find one that's not trusted
    let tools_response = chat.execute_command("/tools")?;
    
    println!("📝 Tools response: {} bytes", tools_response.len());
    println!("📝 TOOLS OUTPUT:");
    println!("{}", tools_response);
    println!("📝 END TOOLS OUTPUT");
    
    // Find a tool that's not trusted
    let mut untrusted_tool: Option<String> = None;
    
    // Look for tools that are "not trusted"
    let lines: Vec<&str> = tools_response.lines().collect();
    for line in lines {
        if line.starts_with("- ") && line.contains("not trusted") {
            // Extract tool name from the line (after "- ")
            if let Some(tool_part) = line.strip_prefix("- ") {
                let parts: Vec<&str> = tool_part.split_whitespace().collect();
                if let Some(tool_name) = parts.first() {
                    untrusted_tool = Some(tool_name.to_string());
                    break;
                }
            }
        }
    }
    
    if let Some(tool_name) = untrusted_tool {
        println!("✅ Found untrusted tool: {}", tool_name);
        
        // Execute trust command
        let trust_command = format!("/tools trust {}", tool_name);
        let trust_response = chat.execute_command(&trust_command)?;
        
        println!("📝 Trust response: {} bytes", trust_response.len());
        println!("📝 TRUST OUTPUT:");
        println!("{}", trust_response);
        println!("📝 END TRUST OUTPUT");
        
        // Verify trust confirmation message
        assert!(trust_response.contains(&tool_name), "Missing trust confirmation message");
        println!("✅ Found trust confirmation message for tool: {}", tool_name);
        
        // Execute untrust command
        let untrust_command = format!("/tools untrust {}", tool_name);
        let untrust_response = chat.execute_command(&untrust_command)?;
        
        println!("📝 Untrust response: {} bytes", untrust_response.len());
        println!("📝 UNTRUST OUTPUT:");
        println!("{}", untrust_response);
        println!("📝 END UNTRUST OUTPUT");
        
        // Verify untrust confirmation message
        let expected_untrust_message = format!("Tool '{}' is set to per-request confirmation.", tool_name);
        assert!(untrust_response.contains(&expected_untrust_message), "Missing untrust confirmation message");
        println!("✅ Found untrust confirmation message for tool: {}", tool_name);
        
        println!("✅ All tools trust/untrust functionality verified!");
    } else {
        println!("ℹ️ No untrusted tools found to test trust command");
    }
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}