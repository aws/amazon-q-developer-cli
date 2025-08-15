use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "tools")]
fn test_tools_trust_all_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /tools trust-all command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    // Execute trust-all command
    let trust_all_response = chat.execute_command("/tools trust-all")?;
    
    println!("📝 Trust-all response: {} bytes", trust_all_response.len());
    println!("📝 TRUST-ALL OUTPUT:");
    println!("{}", trust_all_response);
    println!("📝 END TRUST-ALL OUTPUT");
    
    // Verify trust-all confirmation message
   assert!(trust_all_response.contains("asking for confirmation"), "Missing trust-all confirmation message");
   assert!(trust_all_response.contains("Agents can sometimes do unexpected things so understand the risks."), "Missing risk warning message");
   assert!(trust_all_response.contains("Learn more at https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line-chat-security.html#command-line-chat-trustall-safety"), "Missing documentation link");
   println!("✅ Found trust-all confirmation, risk warning, and documentation link");
    
    // Now check tools list to verify all tools are trusted
    let tools_response = chat.execute_command("/tools")?;
    
    println!("📝 Tools response after trust-all: {} bytes", tools_response.len());
    println!("📝 TOOLS OUTPUT:");
    println!("{}", tools_response);
    println!("📝 END TOOLS OUTPUT");
    
    // Verify that all tools now show "trusted" permission
    assert!(tools_response.contains("trusted"), "Missing trusted tools after trust-all");
    
    // Verify no tools have other permission statuses
    assert!(!tools_response.contains("not trusted"), "Found 'not trusted' tools after trust-all");
    assert!(!tools_response.contains("read-only commands"), "Found 'read-only commands' tools after trust-all");
    println!("✅ Verified all tools are now trusted, no other permission statuses found");
    
    // Count lines with "trusted" to ensure multiple tools are trusted
    let trusted_count = tools_response.matches("trusted").count();
    assert!(trusted_count > 0, "No trusted tools found");
    println!("✅ Found {} instances of 'trusted' in tools list", trusted_count);
    
    println!("✅ All tools trust-all functionality verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}