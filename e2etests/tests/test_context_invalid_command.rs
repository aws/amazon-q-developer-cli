use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
fn test_context_invalid_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /context invalid command...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/context test")?;
    
    println!("📝 Context invalid response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify error message for invalid subcommand
    assert!(response.contains("unrecognized subcommand") && response.contains("test"), "Missing 'unrecognized subcommand' error message");
    println!("✅ Found expected error message for invalid subcommand");
    
    assert!(response.contains("Usage:"), "Missing Usage section");
    assert!(response.contains("/context") && response.contains("<COMMAND>"), "Missing /context command in usage");
    println!("✅ Found Usage section with /context command");
    
    assert!(response.contains("For more information") && response.contains("--help"), "Missing help information");
    println!("✅ Found help information");
    
    assert!(response.contains("Commands:"), "Missing Commands section");
    assert!(response.contains("show"), "Missing show command");
    assert!(response.contains("add"), "Missing add command");
    assert!(response.contains("remove"), "Missing remove command");
    assert!(response.contains("clear"), "Missing clear command");
    assert!(response.contains("help"), "Missing help command");
    println!("✅ Found Commands section with all subcommands");
    
    assert!(response.contains("Display the context rule configuration and matched files"), "Missing show command description");
    assert!(response.contains("Add context rules (filenames or glob patterns)"), "Missing add command description");
    assert!(response.contains("Remove specified rules"), "Missing remove command description");
    assert!(response.contains("Remove all rules"), "Missing clear command description");
    println!("✅ Found all command descriptions");
    
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-h"), "Missing -h flag");
    assert!(response.contains("--help"), "Missing --help flag");
    assert!(response.contains("Print help"), "Missing help description");
    println!("✅ Found Options section with -h, --help flags");
    
    println!("✅ All context invalid command content verified!");
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}
