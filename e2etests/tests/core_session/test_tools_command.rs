#[allow(unused_imports)]
use q_cli_e2e_tests::{get_chat_session, cleanup_if_last_test};
use std::sync::atomic::AtomicUsize;

#[allow(dead_code)]
static TEST_COUNT: AtomicUsize = AtomicUsize::new(0);

// List of covered tests
#[allow(dead_code)]
const TEST_NAMES: &[&str] = &[
    "test_tools_command",
    "test_tools_help_command",
    "test_tools_trust_all_command",
    "test_tools_trust_all_help_command",
    "test_tools_reset_help_command",
    "test_tools_trust_command",
    "test_tools_trust_help_command",
    "test_tools_untrust_help_command",
    "test_tools_schema_help_command",
];
#[allow(dead_code)]
const TOTAL_TESTS: usize = TEST_NAMES.len();

#[test]
#[cfg(all(feature = "tools", feature = "sanity"))]
fn test_tools_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /tools command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

    let response = chat.execute_command("/tools")?;
    
    println!("📝 Tools response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify tools content structure
    assert!(response.contains("Tool"), "Missing Tool header");
    assert!(response.contains("Permission"), "Missing Permission header");
    println!("✅ Found tools table with Tool and Permission columns");
    
    assert!(response.contains("Built-in:"), "Missing Built-in section");
    println!("✅ Found Built-in tools section");
    
    // Verify some expected built-in tools
    assert!(response.contains("execute_bash"), "Missing execute_bash tool");
    assert!(response.contains("fs_read"), "Missing fs_read tool");
    assert!(response.contains("fs_write"), "Missing fs_write tool");
    assert!(response.contains("use_aws"), "Missing use_aws tool");
    println!("✅ Verified core built-in tools: execute_bash, fs_read, fs_write, use_aws");
    
    // Check for MCP tools section if present
    if response.contains("amzn-mcp (MCP):") {
        println!("✅ Found MCP tools section with Amazon-specific tools");
        assert!(response.contains("not trusted") || response.contains("trusted"), "Missing permission status");
        println!("✅ Verified permission status indicators (trusted/not trusted)");
        
        // Count some MCP tools
        let mcp_tools = ["andes", "cradle", "datanet", "read_quip", "taskei_get_task"];
        let found_tools: Vec<&str> = mcp_tools.iter().filter(|&&tool| response.contains(tool)).copied().collect();
        println!("✅ Found {} MCP tools including: {:?}", found_tools.len(), found_tools);
    }
    
    println!("✅ All tools content verified!");
    
    println!("✅ /tools command executed successfully");

    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "tools", feature = "sanity"))]
fn test_tools_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /tools --help command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

    let response = chat.execute_command("/tools --help")?;
    
    println!("📝 Tools help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify Usage section
    assert!(response.contains("Usage:") && response.contains("/tools") && response.contains("[COMMAND]"), "Missing Usage section");
    println!("✅ Found usage format");
    println!("✅ Found usage format");
    
    // Verify Commands section
    assert!(response.contains("Commands:"), "Missing Commands section");
    assert!(response.contains("schema"), "Missing schema command");
    assert!(response.contains("trust"), "Missing trust command");
    assert!(response.contains("untrust"), "Missing untrust command");
    assert!(response.contains("trust-all"), "Missing trust-all command");
    assert!(response.contains("reset"), "Missing reset command");
    assert!(response.contains("help"), "Missing help command");
    println!("✅ Found all commands: schema, trust, untrust, trust-all, reset, help");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-h") &&  response.contains("--help"), "Missing -h, --help flags");
    println!("✅ Found Options section with help flags");
    
    println!("✅ All tools help content verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "tools", feature = "sanity"))]
fn test_tools_trust_all_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /tools trust-all command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

    // Execute trust-all command
    let trust_all_response = chat.execute_command("/tools trust-all")?;
    
    println!("📝 Trust-all response: {} bytes", trust_all_response.len());
    println!("📝 TRUST-ALL OUTPUT:");
    println!("{}", trust_all_response);
    println!("📝 END TRUST-ALL OUTPUT");
    
    // Verify that all tools now show "trusted" permission
    assert!(trust_all_response.contains("All tools") && trust_all_response.contains("trusted"), "Missing trusted tools after trust-all");
    println!("✅ trust-all confirmation message!!");
    
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
    
    // Execute reset command
    let reset_response = chat.execute_command("/tools reset")?;
    
    println!("📝 Reset response: {} bytes", reset_response.len());
    println!("📝 RESET OUTPUT:");
    println!("{}", reset_response);
    println!("📝 END RESET OUTPUT");
    
    // Verify reset confirmation message
    assert!(reset_response.contains("Reset") && reset_response.contains("permission"), "Missing reset confirmation message");
    println!("✅ Found reset confirmation message");
    
    // Now check tools list to verify tools have mixed permissions
    let tools_response = chat.execute_command("/tools")?;
    
    println!("📝 Tools response after reset: {} bytes", tools_response.len());
    println!("📝 TOOLS OUTPUT:");
    println!("{}", tools_response);
    println!("📝 END TOOLS OUTPUT");
    
    // Verify that tools have all permission types
    assert!(tools_response.contains("trusted"), "Missing trusted tools");
    assert!(tools_response.contains("not trusted"), "Missing not trusted tools");
    assert!(tools_response.contains("read-only commands"), "Missing read-only commands tools");
    println!("✅ Found all permission types after reset");
    
    println!("✅ All tools reset functionality verified!");

    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "tools", feature = "sanity"))]
fn test_tools_trust_all_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /tools trust-all --help command...");
  
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

    let response = chat.execute_command("/tools trust-all --help")?;
    
    println!("📝 Tools trust-all help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    
    // Verify usage format
    assert!(response.contains("Usage:") && response.contains("/tools trust-all"), "Missing usage format");
    println!("✅ Found usage format");
    
    // Verify options section
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-h") && response.contains("--help"), "Missing help option");
    println!("✅ Found options section with help flag");
    
    println!("✅ All tools trust-all help functionality verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "tools", feature = "sanity"))]
fn test_tools_reset_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /tools reset --help command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

    let response = chat.execute_command("/tools reset --help")?;
    
    println!("📝 Tools reset help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify usage format
    assert!(response.contains("Usage:") && response.contains("/tools reset"), "Missing usage format");
    println!("✅ Found usage format");
    
    // Verify options section
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-h") && response.contains("--help"), "Missing help option");
    println!("✅ Found options section with help flag");
    
    println!("✅ All tools reset help functionality verified!");
     
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "tools", feature = "sanity"))]
fn test_tools_trust_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /tools trust command...");
  
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

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
        let expected_untrust_message = format!("Tool '{}' is", tool_name);
        assert!(untrust_response.contains(&expected_untrust_message), "Missing untrust confirmation message");
        println!("✅ Found untrust confirmation message for tool: {}", tool_name);
        
        println!("✅ All tools trust/untrust functionality verified!");
    } else {
        println!("ℹ️ No untrusted tools found to test trust command");
    }
  
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "tools", feature = "sanity"))]
fn test_tools_trust_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /tools trust --help command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();
    
    let response = chat.execute_command("/tools trust --help")?;
    
    println!("📝 Tools trust help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify usage format
    assert!(response.contains("Usage:") && response.contains("/tools trust") && response.contains("<TOOL_NAMES>"), "Missing usage format");
    println!("✅ Found usage format");
    
    // Verify arguments section
    assert!(response.contains("Arguments:") && response.contains("<TOOL_NAMES>"), "Missing Arguments section");
    println!("✅ Found arguments section");
    
    // Verify options section
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-h") && response.contains("--help"), "Missing help option");
    println!("✅ Found options section with help flag");
    
    println!("✅ All tools trust help functionality verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "tools", feature = "sanity"))]
fn test_tools_untrust_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /tools untrust --help command...");

    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

    let response = chat.execute_command("/tools untrust --help")?;
    
    println!("📝 Tools untrust help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify usage format
    assert!(response.contains("Usage:") && response.contains("/tools untrust") && response.contains("<TOOL_NAMES>"), "Missing usage format");
    println!("✅ Found usage format");
    
    // Verify arguments section
    assert!(response.contains("Arguments:") && response.contains("<TOOL_NAMES>"), "Missing Arguments section");
    println!("✅ Found arguments section");
    
    // Verify options section
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-h") && response.contains("--help"), "Missing help option");
    println!("✅ Found options section with help flag");
    
    println!("✅ All tools untrust help functionality verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "tools", feature = "sanity"))]
fn test_tools_schema_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /tools schema --help command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

    let response = chat.execute_command("/tools schema --help")?;
    
    println!("📝 Tools schema help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Verify usage format
    assert!(response.contains("Usage:") && response.contains("/tools schema"), "Missing usage format");
    println!("✅ Found usage format");
    
    // Verify options section
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-h") && response.contains("--help"), "Missing help option");
    println!("✅ Found options section with help flag");
    
    println!("✅ All tools schema help functionality verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}
//TODO: As response not giving full content , need to check this.
/*#[test]
#[cfg(feature = "tools")]
fn test_tools_schema_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /tools schema command...");
  
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

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
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}*/
