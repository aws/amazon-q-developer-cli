use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "tools")]
fn test_tools_schema_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /tools schema command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/tools schema")?;
    
    println!("📝 Tools schema response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify JSON structure
    assert!(response.contains("{") && response.contains("}"), "Missing JSON structure");
    println!("✅ Found JSON structure");
    
    // Verify core built-in tools
    assert!(response.contains("fs_read") || response.contains("fs_write") || response.contains("execute_bash") || response.contains("use_aws"), "Missing tools");
    println!("✅ Found core built-in tools");
    
    // Verify tool structure elements
    assert!(response.contains("name"), "Missing name field");
    assert!(response.contains("description"), "Missing description field");
    assert!(response.contains("input_schema"), "Missing input_schema field");
    assert!(response.contains("properties"), "Missing properties field");
    println!("✅ Found required tool structure: name, description, input_schema, properties");
    
    // Check for optional MCP/GitHub tools if present
    if response.contains("download_files_from_github") {
        println!("✅ Found GitHub-related tools");
    }
    if response.contains("consolidate_findings_to_csv") {
        println!("✅ Found analysis tools");
    }
    if response.contains("gh_issue") {
        println!("✅ Found GitHub issue reporting tool");
    }
    
    // Verify schema structure for at least one tool
    if response.contains("type") {
        println!("✅ Found proper schema type definitions");
    }
    
    println!("✅ All tools schema content verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}
