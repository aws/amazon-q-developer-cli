use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "agent")]
fn test_agent_list_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /agent list command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/agent list")?;
    
    println!("📝 Agent list response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify the response contains the expected agent
    assert!(response.contains("q_cli_default"), "Missing q_cli_default agent");
    println!("✅ Found q_cli_default agent in list");
    
    // Verify the response format (should show agent with bullet point)
    assert!(response.contains("* q_cli_default"), "Missing bullet point format for q_cli_default");
    println!("✅ Verified bullet point format for agent list");
    
    // Verify the permission prompt appears
    //TODO:"This option only shows on first time"
    // assert!(response.contains("Allow this action? Use 't' to trust (always allow) this tool for the session. [y/n/t]:"), "Missing permission prompt");
    // println!("✅ Found permission prompt for tool usage");
    
    println!("✅ /agent list command executed successfully");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}