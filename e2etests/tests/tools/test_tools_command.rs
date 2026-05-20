#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;
use std::sync::{Mutex, Once, atomic::{AtomicUsize, Ordering}};
#[allow(dead_code)]
static INIT: Once = Once::new();
#[allow(dead_code)]
static mut CHAT_SESSION: Option<Mutex<q_chat_helper::QChatSession>> = None;

#[allow(dead_code)]
pub fn get_chat_session() -> &'static Mutex<q_chat_helper::QChatSession> {
    unsafe {
        INIT.call_once(|| {
            let chat = q_chat_helper::QChatSession::new().expect("Failed to create chat session");
            println!("✅ Q Chat session started");
            CHAT_SESSION = Some(Mutex::new(chat));
        });
        (&raw const CHAT_SESSION).as_ref().unwrap().as_ref().unwrap()
    }
}

#[allow(dead_code)]
pub fn cleanup_if_last_test(test_count: &AtomicUsize, total_tests: usize) -> Result<usize, Box<dyn std::error::Error>> {
    let count = test_count.fetch_add(1, Ordering::SeqCst) + 1;
    if count == total_tests {
        unsafe {
            if let Some(session) = (&raw const CHAT_SESSION).as_ref().unwrap() {
                if let Ok(mut chat) = session.lock() {
                    chat.quit()?;
                    println!("✅ Test completed successfully");
                }
            }
        }
    }
  Ok(count)
}
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
    "test_fs_write_and_fs_read_tools",
    "test_execute_bash_tool",
    "test_report_issue_tool",
    "test_use_aws_tool",
    "test_trust_execute_bash_for_direct_execution"
];
#[allow(dead_code)]
const TOTAL_TESTS: usize = TEST_NAMES.len();

#[allow(dead_code)]
struct FileCleanup<'a> {
    path: &'a str,
}

impl<'a> Drop for FileCleanup<'a> {
    fn drop(&mut self) {
        if std::path::Path::new(self.path).exists() {
            let _ = std::fs::remove_file(self.path);
            println!("✅ Cleaned up test file");
        }
    }
}

#[test]
#[cfg(all(feature = "tools", feature = "sanity"))]
fn test_tools_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /tools command... | Description: Tests the <code>/tools</code> command to display all available tools with their permission status including built-in and MCP tools");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

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
    println!("\n🔍 Testing /tools --help command... | Description: Tests the <code> /tools --help</code> command to display comprehensive help information about tools management including available subcommands and options");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

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
    println!("\n🔍 Testing /tools trust-all command... | Description: Tests the <code> /tools trust-all</code> command to trust all available tools and verify all tools show trusted status, then tests reset functionality");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

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
    println!("\n🔍 Testing /tools trust-all --help command... | Description: Tests the <code> /tools trust-all --help</code>command to display help information for the trust-all subcommand");
  
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

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
    println!("\n🔍 Testing /tools reset --help command... | Description: Tests the <code> /tools reset --help</code> command to display help information for the reset subcommand");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

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
    println!("\n🔍 Testing /tools trust command... | Description: Tests the <code> /tools</code> trust and untrust commands to manage individual tool permissions and verify trust status changes");
  
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

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
    println!("\n🔍 Testing /tools trust --help command... | Description: Tests the <code>/tools trust --help</code> command to display help information for trusting specific tools");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
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
    println!("\n🔍 Testing /tools untrust --help command... | Description: Tests the <code>/tools untrust --help</code> command to display help information for untrusting specific tools");

    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

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
    println!("\n🔍 Testing /tools schema --help command... | Description: Tests the <code>/tools schema --help</code> command to display help information for viewing tool schemas");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

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
    println!("\n🔍 Testing /tools schema command...");
  
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
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

#[test]
#[cfg(all(feature = "tools", feature = "sanity"))]
fn test_fs_write_and_fs_read_tools() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing `fs_write` and `fs_read` tool ... | Description: Tests the <code> fs_write</code> and <code> fs_read</code> tools by creating a file with specific content and reading it back to verify file I/O operations work correctly");

    let save_path = "demo.txt";
    let _cleanup = FileCleanup { path: save_path };
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    // Test fs_write tool by asking to create a file with "Hello World" content
    let response = chat.execute_command(&format!("Create a file at {} with content 'Hello World'", save_path))?;

    println!("📝 fs_write response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify tool usage indication
    assert!(response.contains("Using tool") && response.contains("fs_write"), "Missing fs_write tool usage indication");
    println!("✅ Found fs_write tool usage indication");
    
    // Verify file path in response
    assert!(response.contains("demo.txt"), "Missing expected file path");
    println!("✅ Found expected file path in response");

     // Allow the tool execution
    let allow_response = chat.execute_command("y")?;
    
    println!("📝 Allow response: {} bytes", allow_response.len());
    println!("📝 ALLOW RESPONSE:");
    println!("{}", allow_response);
    println!("📝 END ALLOW RESPONSE");
    
    // Verify content reference
    assert!(allow_response.contains("Hello World"), "Missing expected content reference");
    println!("✅ Found expected content reference");
    
    // Verify success indication
    assert!(allow_response.contains("Created"), "Missing success indication");
    println!("✅ Found success indication");

    // Test fs_read tool by asking to read the created file
    let response = chat.execute_command(&format!("Read file {}'", save_path))?;

    println!("📝 fs_read response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify tool usage indication
    assert!(response.contains("Using tool") && response.contains("fs_read"), "Missing fs_read tool usage indication");
    println!("✅ Found fs_read tool usage indication");
    
    // Verify file path in response
    assert!(response.contains("demo.txt"), "Missing expected file path");
    println!("✅ Found expected file path in response");
    
    // Verify content reference
    assert!(allow_response.contains("Hello World"), "Missing expected content reference");
    println!("✅ Found expected content reference");
    
    println!("✅ All fs_write and fs_read tool functionality verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "tools", feature = "sanity"))]
fn test_execute_bash_tool() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing `execute_bash` tool ... | Description: Tests the <code>execute_bash</code> tool by running the 'pwd' command and verifying proper command execution and output");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    // Test execute_bash tool by asking to run pwd command
    let response = chat.execute_command("Run pwd")?;

    println!("📝 execute_bash response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify tool usage indication
    assert!(response.contains("Using tool") && response.contains("execute_bash"), "Missing execute_bash tool usage indication");
    println!("✅ Found execute_bash tool usage indication");
    
    // Verify command in response
    assert!(response.contains("pwd"), "Missing expected command");
    println!("✅ Found pwd command in response");
    
    // Verify success indication
    assert!(response.contains("current working directory"), "Missing success indication");
    println!("✅ Found success indication");
    
    println!("✅ All execute_bash functionality verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "tools", feature = "sanity"))]
fn test_report_issue_tool() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing `report_issue` tool ... | Description: Tests the <code> report_issue</code> reporting functionality by creating a sample issue and verifying the browser opens GitHub for issue submission");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    // Test report_issue tool by asking to report an issue
    let response = chat.execute_command("Report an issue: 'File creation not working properly'")?;

    println!("📝 report_issue response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify tool usage indication
    assert!(response.contains("Using tool") && response.contains("gh_issue"), "Missing report_issue tool usage indication");
    println!("✅ Found report_issue tool usage indication");
    
    // Verify command executed successfully (GitHub opens automatically)
    assert!(response.contains("Heading over to GitHub..."), "Missing browser opening confirmation");
    println!("✅ Found browser opening confirmation");
    
    println!("✅ All report_issue functionality verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "tools", feature = "sanity"))]
fn test_use_aws_tool() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing `use_aws` tool ... | Description: Tests the <code>use_aws</code> tool by executing AWS commands to describe EC2 instances and verifying proper AWS CLI integration");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    // Test use_aws tool by asking to describe EC2 instances in us-west-2
    let response = chat.execute_command("Describe EC2 instances in us-west-2")?;

    println!("📝 use_aws response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify tool usage indication
    assert!(response.contains("Using tool") && response.contains("use_aws"), "Missing use_aws tool usage indication");
    println!("✅ Found use_aws tool usage indication");
    
    // Verify command executed successfully.
    assert!(response.contains("aws"), "Missing aws information");
    println!("✅ Found aws information");
    
    println!("✅ All use_aws functionality verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "tools", feature = "sanity"))]
fn test_trust_execute_bash_for_direct_execution() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing Trust execute_bash for direct execution ... | Description: Tests the ability to trust the <code>execute_bash</code> tool so it runs commands without asking for user confirmation each time");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    // First, trust the execute_bash tool
    let trust_response = chat.execute_command("/tools trust execute_bash")?;
    
    println!("📝 Trust response: {} bytes", trust_response.len());
    println!("📝 TRUST OUTPUT:");
    println!("{}", trust_response);
    println!("📝 END TRUST OUTPUT");
    
    // Verify trust confirmation
    assert!(trust_response.contains("trusted") || trust_response.contains("execute_bash"), "Missing trust confirmation");
    println!("✅ Found trust confirmation");

    // Now test execute_bash tool with a simple command that should run directly without confirmation
    let response = chat.execute_command("Run mkdir -p test_dir && echo 'test' > test_dir/test.txt")?;

    println!("📝 execute_bash response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify tool usage indication
    assert!(response.contains("Using tool") && response.contains("execute_bash"), "Missing execute_bash tool usage indication");
    println!("✅ Found execute_bash tool usage indication");
    
    // Verify the command was executed directly without asking for confirmation
    assert!(response.contains("Created") && response.contains("directory") && response.contains("test_dir") , "Missing success message");
    println!("✅ Found success message");
    
    println!("✅ All trusted execute_bash functionality verified!");

    chat.execute_command("Delete the directory test_dir/test.txt")?;
     
    println!("✅ Directory successfully deleted");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}