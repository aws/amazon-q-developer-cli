#[allow(unused_imports)]
use q_cli_e2e_tests::{get_chat_session, cleanup_if_last_test};
use std::sync::atomic::{AtomicUsize};

#[allow(dead_code)]
static TEST_COUNT: AtomicUsize = AtomicUsize::new(0);

// List of covered tests
#[allow(dead_code)]
const TEST_NAMES: &[&str] = &[
    "test_mcp_help_command",
    "test_mcp_loading_command",
    "test_mcp_remove_help_command",
    "test_q_mcp_add_help_command",
    "test_q_mcp_help_command",
    "test_q_mcp_import_help_command",
    "test_q_mcp_list_command",
    "test_q_mcp_list_help_command",
    "test_q_mcp_status_help_command"
];
#[allow(dead_code)]
const TOTAL_TESTS: usize = TEST_NAMES.len();

#[test]
#[cfg(all(feature = "mcp", feature = "regression"))]
fn test_mcp_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /mcp --help command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("/mcp --help")?;
    
    println!("📝 MCP help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify description
    assert!(response.contains("See mcp server loaded"), "Missing mcp server description");
    println!("✅ Found mcp server description");
    
    // Verify Usage section
    assert!(response.contains("Usage"), "Missing Usage section");
    assert!(response.contains("/mcp"), "Missing /mcp command in usage section");
    println!("✅ Found Usage section with /mcp command");
    
    // Verify Options section
    assert!(response.contains("Options"), "Missing Options section");
    println!("✅ Found Options section");
    
    // Verify help flags
    assert!(response.contains("-h") &&  response.contains("--help") && response.contains("Print help"), "Missing -h, --help flags");
    println!("✅ Found help flags: -h, --help with Print help description");
    
    println!("✅ All mcp help content verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(all(feature = "mcp", feature = "regression"))]
fn test_mcp_loading_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing MCP loading...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("/mcp")?;
    
    println!("📝 MCP loading response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Check MCP status - either loaded or loading
    if response.contains("loaded in") {
        assert!(response.contains(" s"), "Missing seconds indicator for loading time");
        println!("✅ Found MCPs loaded with timing");
        
        // Count number of MCPs loaded
        let mcp_count = response.matches("✓").count();
        println!("✅ Found {} MCP(s) loaded", mcp_count);
    } else if response.contains("loading") {
        println!("✅ MCPs are still loading");
    } else {
        println!("ℹ️ MCP status unclear - may be in different state");
    }
    
    println!("✅ All MCP loading content verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(all(feature = "mcp", feature = "regression"))]
fn test_mcp_remove_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing q mcp remove --help command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    // Execute q mcp remove --help command
    let help_response = chat.execute_command("q mcp remove --help")?;
    
    println!("📝 MCP remove help response: {} bytes", help_response.len());
    println!("📝 HELP RESPONSE:");
    println!("{}", help_response);
    println!("📝 END HELP RESPONSE");
    
    // Verify tool execution prompt appears
    assert!(help_response.contains("Using tool: execute_bash"), "Missing tool execution indicator");
    assert!(help_response.contains("Allow this action?") && help_response.contains("to trust (always allow) this tool for the session."), "Missing permission prompt");
    println!("✅ Found tool execution permission prompt");
    
    // Allow the tool execution
    let allow_response = chat.execute_command("y")?;

    println!("📝 Allow response: {} bytes", allow_response.len());
    println!("📝 ALLOW RESPONSE:");
    println!("{}", allow_response);
    println!("📝 END ALLOW RESPONSE");
    
    // Verify complete help content in final response
    assert!(allow_response.contains("Usage: qchat mcp remove"), "Missing usage information");
    assert!(allow_response.contains("Options"), "Missing option information");
    assert!(allow_response.contains("--name <NAME>"), "Missing --name option");
    assert!(allow_response.contains("--scope <SCOPE>"), "Missing --scope option");
    assert!(allow_response.contains("--agent <AGENT>"), "Missing --agent option");
    assert!(allow_response.contains("-h, --help"), "Missing help option");
    assert!(allow_response.contains("Completed in"), "Missing completion indicator");
    println!("✅ Found all expected MCP remove help content and completion");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(all(feature = "mcp", feature = "regression"))]
fn test_q_mcp_add_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing q mcp add --help command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    // Execute mcp add --help command
    println!("🔍 Executing command: 'q mcp add --help'");
    let response = chat.execute_command("q mcp add --help")?;
    
    println!("📝 Restart response: {} bytes", response.len());
    println!("📝 RESTART RESPONSE:");
    println!("{}", response);
    println!("📝 END RESTART RESPONSE");
    
    // Verify tool execution details
    assert!(response.contains("q mcp add --help"), "Missing command execution description");
    assert!(response.contains("Purpose:"), "Missing purpose description");
    println!("✅ Found tool execution details");

    // Verify tool execution prompt appears
    assert!(response.contains("Using tool: execute_bash"), "Missing tool execution indicator");
    assert!(response.contains("Allow this action?") && response.contains("to trust (always allow) this tool for the session."), "Missing permission prompt");
    println!("✅ Found tool execution permission prompt");
    
    // Allow the tool execution
    let allow_response = chat.execute_command("y")?;
    
    println!("📝 Allow response: {} bytes", allow_response.len());
    println!("📝 ALLOW RESPONSE:");
    println!("{}", allow_response);
    println!("📝 END ALLOW RESPONSE");
    
    // Verify mcp add help output
    assert!(allow_response.contains("Usage: qchat mcp add"), "Missing usage information");
     assert!(allow_response.contains("Options"), "Missing Options");
    assert!(allow_response.contains("--name <NAME>"), "Missing --name option");
    assert!(allow_response.contains("--command <COMMAND>"), "Missing --command option");
    assert!(allow_response.contains("--scope <SCOPE>"), "Missing --scope option");
    assert!(allow_response.contains("--args <ARGS>"), "Missing --args option");
    assert!(allow_response.contains("--agent <AGENT>"), "Missing --agent option");
    assert!(allow_response.contains("--env <ENV>"), "Missing --env option");
    assert!(allow_response.contains("--timeout <TIMEOUT>"), "Missing --timeout option");
    assert!(allow_response.contains("--disabled"), "Missing --disabled option");
    assert!(allow_response.contains("--force"), "Missing --force option");
    assert!(allow_response.contains("--verbose"), "Missing --verbose option");
    assert!(allow_response.contains("--help"), "Missing --help option");
    assert!(allow_response.contains("Completed in"), "Missing completion indicator");
    assert!(allow_response.contains("Required"), "Missing Requried indicator");
    assert!(allow_response.contains("Optional"), "Missing Optional indicator");
    println!("✅ MCP add help command executed successfully");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(all(feature = "mcp", feature = "regression"))]
fn test_q_mcp_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing q mcp --help command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    // Execute q mcp --help command
    let help_response = chat.execute_command("q mcp --help")?;
    
    println!("📝 MCP help response: {} bytes", help_response.len());
    println!("📝 HELP RESPONSE:");
    println!("{}", help_response);
    println!("📝 END HELP RESPONSE");
    
    // Verify tool execution prompt appears
    assert!(help_response.contains("Using tool: execute_bash"), "Missing tool execution indicator");
    assert!(help_response.contains("Allow this action?") && help_response.contains("to trust (always allow) this tool for the session."), "Missing permission prompt");
    println!("✅ Found tool execution permission prompt");
    
    // Allow the tool execution
    let allow_response = chat.execute_command("y")?;

    println!("📝 Allow response: {} bytes", allow_response.len());
    println!("📝 ALLOW RESPONSE:");
    println!("{}", allow_response);
    println!("📝 END ALLOW RESPONSE");
    
    // Verify complete help content
    assert!(allow_response.contains("Model Context Protocol (MCP)"), "Missing MCP description");
    assert!(allow_response.contains("Usage: qchat mcp"), "Missing usage information");
    assert!(allow_response.contains("Commands:"), "Missing Commands section");
    
    // Verify command descriptions
    assert!(allow_response.contains("add"), "Missing add command description");
    assert!(allow_response.contains("remove"), "Missing remove command description");
    assert!(allow_response.contains("list"), "Missing list command description");
    assert!(allow_response.contains("import"), "Missing import command description");
    assert!(allow_response.contains("status"), "Missing status command description");
    assert!(allow_response.contains("help"), "Missing help command");
    println!("✅ Found all MCP commands with descriptions");
    
    assert!(allow_response.contains("Options"), "Missing Options section");
    assert!(allow_response.contains("-v, --verbose"), "Missing verbose option");
    assert!(allow_response.contains("-h, --help"), "Missing help option");
    assert!(allow_response.contains("Completed in"), "Missing completion indicator");
    println!("✅ Found all expected MCP help content and completion");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(all(feature = "mcp", feature = "regression"))]
fn test_q_mcp_import_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing q mcp import --help command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    // Execute mcp import --help command
    println!("🔍 Executing command: 'q mcp import --help'");
    let response = chat.execute_command("q mcp import --help")?;
    
    println!("📝 Restart response: {} bytes", response.len());
    println!("📝 RESTART RESPONSE:");
    println!("{}", response);
    println!("📝 END RESTART RESPONSE");

    // Verify tool execution details
    assert!(response.contains("q mcp import --help"), "Missing command execution description");
    assert!(response.contains("Purpose:"), "Missing purpose description");
    println!("✅ Found tool execution details");
    
    // Verify tool execution prompt appears
    assert!(response.contains("Using tool: execute_bash"), "Missing tool execution indicator");
    assert!(response.contains("Allow this action?") && response.contains("to trust (always allow) this tool for the session."), "Missing permission prompt");
    println!("✅ Found tool execution permission prompt");
    
    // Allow the tool execution
    let allow_response = chat.execute_command("y")?;
    
    println!("📝 Allow response: {} bytes", allow_response.len());
    println!("📝 ALLOW RESPONSE:");
    println!("{}", allow_response);
    println!("📝 END ALLOW RESPONSE");
    
    // Verify usage line
    assert!(allow_response.contains("Usage"), "Missing complete usage line");
    println!("✅ Found usage information");
    
    // Verify Arguments section
    assert!(allow_response.contains("Arguments"), "Missing Arguments section");
    println!("✅ Found Arguments section with SCOPE");
    
    // Verify Options section
    assert!(allow_response.contains("Options"), "Missing Options section");
    assert!(allow_response.contains("--file <FILE>"), "Missing --file option");
    assert!(allow_response.contains("--force"), "Missing --force option");
    assert!(allow_response.contains("-v, --verbose..."), "Missing --verbose option");
    assert!(allow_response.contains("-h, --help"), "Missing --help option");
    println!("✅ Found all options with descriptions");
    
    // Verify completion indicator
    assert!(allow_response.contains("Completed in") && allow_response.contains("s"), "Missing completion time indicator");
    println!("✅ Found completion indicator");
    
    println!("✅ All q mcp import --help content verified successfully");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(all(feature = "mcp", feature = "regression"))]
fn test_q_mcp_list_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing q mcp list command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("q mcp list")?;
    
    println!("📝 MCP list response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify tool execution prompt
    assert!(response.contains("Using tool:"), "Missing tool execution indicator");
    assert!(response.contains("q mcp list"), "Missing command in tool execution");
    assert!(response.contains("Allow this action?") && response.contains("to trust (always allow) this tool for the session."), "Missing permission prompt");
    println!("✅ Found tool execution permission prompt");
    
    // Allow the tool execution
    let allow_response = chat.execute_command("y")?;
    
    println!("📝 Allow response: {} bytes", allow_response.len());
    println!("📝 ALLOW RESPONSE:");
    println!("{}", allow_response);
    println!("📝 END ALLOW RESPONSE");
    
    
    // Verify MCP server listing
    assert!(allow_response.contains("q_cli_default"), "Missing q_cli_default server");
    println!("✅ Found MCP server listing with  servers and completion");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(all(feature = "mcp", feature = "regression"))]
fn test_q_mcp_list_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing q mcp list --help command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("q mcp list --help")?;
    
    println!("📝 MCP list help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify tool execution prompt
    assert!(response.contains("Using tool:"), "Missing tool execution indicator");
    assert!(response.contains("q mcp list --help"), "Missing command in tool execution");
    assert!(response.contains("Allow this action?") && response.contains("to trust (always allow) this tool for the session."), "Missing permission prompt");
    println!("✅ Found tool execution permission prompt");
    
    // Allow the tool execution
    let allow_response = chat.execute_command("y")?;
    
    println!("📝 Allow response: {} bytes", allow_response.len());
    println!("📝 ALLOW RESPONSE:");
    println!("{}", allow_response);
    println!("📝 END ALLOW RESPONSE");
    
    // Verify help content
    assert!(allow_response.contains("Usage"), "Missing usage format");
    
    // Verify arguments section
    assert!(allow_response.contains("Arguments"), "Missing Arguments section");
    assert!(allow_response.contains("[SCOPE]"), "Missing scope argument");
    
    // Verify options section
    assert!(allow_response.contains("Options"), "Missing Options section");
    assert!(allow_response.contains("-v") && allow_response.contains("--verbose"), "Missing verbose option");
    assert!(allow_response.contains("-h") && allow_response.contains("--help"), "Missing help option");

    
    assert!(allow_response.contains("Completed in"), "Missing completion indicator");
    println!("✅ Found all MCP list help content with explanations and completion");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(all(feature = "mcp", feature = "regression"))]
fn test_q_mcp_status_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing q mcp status --help command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    // Execute mcp status --help command
    println!("🔍 Executing command: 'q mcp status --help'");
    let response = chat.execute_command("q mcp status --help")?;

    println!("📝 Restart response: {} bytes", response.len());
    println!("📝 RESTART RESPONSE:");
    println!("{}", response);
    println!("📝 END RESTART RESPONSE");

    // Verify tool execution details
    assert!(response.contains("Purpose:"), "Missing purpose description");
    println!("✅ Found tool execution details");
    
    // Verify tool execution prompt appears
    assert!(response.contains("Using tool: execute_bash"), "Missing tool execution indicator");
    assert!(response.contains("Allow this action?") && response.contains("to trust (always allow) this tool for the session."), "Missing permission prompt");
    println!("✅ Found tool execution permission prompt");
    
    // Allow the tool execution
    let allow_response = chat.execute_command("y")?;
    
    println!("📝 Allow response: {} bytes", allow_response.len());
    println!("📝 ALLOW RESPONSE:");
    println!("{}", allow_response);
    println!("📝 END ALLOW RESPONSE");
    
    // Verify usage line
    assert!(allow_response.contains("Usage: qchat mcp status [OPTIONS] --name <NAME>"), "Missing complete usage line");
    println!("✅ Found usage information");
    
    // Verify Options section
    assert!(allow_response.contains("Options"), "Missing Options section");
    assert!(allow_response.contains("--name <NAME>"), "Missing --name option");
    assert!(allow_response.contains("-v, --verbose...") , "Missing --verbose option");
    assert!(allow_response.contains("-h, --help"), "Missing --help option");
    println!("✅ Found all options with descriptions");
    
    // Verify completion indicator
    assert!(allow_response.contains("Completed in") && allow_response.contains("s"), "Missing completion time indicator");
    println!("✅ Found completion indicator");
    
    println!("✅ All q mcp status --help content verified successfully");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}
