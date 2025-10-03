use q_cli_e2e_tests::q_chat_helper;

/// Tests the /agent command without subcommands to display help information
/// Verifies agent management description, usage, available subcommands, and options
#[test]
#[cfg(all(feature = "agent", feature = "sanity"))]
fn agent_without_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /agent command... | Description: Tests the <code> /agent</code> command without subcommands to display help information. Verifies agent management description, usage, available subcommands, and options");
    
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("/agent")?;
    
    println!("📝 Agent response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    assert!(response.contains("Manage agents"), "Missing 'Manage agents' description");
    assert!(response.contains("Usage:"), "Missing usage information");
    assert!(response.contains("/agent"), "Missing agent command");
    assert!(response.contains("<COMMAND>"), "Missing command placeholder");
    println!("✅ Found agent command description and usage");
    
    assert!(response.contains("Commands:"), "Missing Commands section");
    assert!(response.contains("list"), "Missing list subcommand");
    assert!(response.contains("create"), "Missing create subcommand");
    assert!(response.contains("schema"), "Missing schema subcommand");
    assert!(response.contains("set-default"), "Missing set-default subcommand");
    assert!(response.contains("help"), "Missing help subcommand");
    println!("✅ Verified all agent subcommands: list, create, schema, set-default, help");
    
    assert!(response.contains("List all available agents"), "Missing list command description");
    assert!(response.contains("Create a new agent"), "Missing create command description");
    assert!(response.contains("Show agent config schema"), "Missing schema command description");
    assert!(response.contains("Define a default agent"), "Missing set-default command description");
    println!("✅ Verified command descriptions");
    
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-h"), "Missing short help option");
    assert!(response.contains("--help"), "Missing long help option");
    println!("✅ Found options section with help flag");
    
    println!("✅ /agent command executed successfully");
    
    // Release the lock before cleanup
    drop(chat);
    
    Ok(())
}

/// Tests the /agent create command to create a new agent with specified name
/// Verifies agent creation process, file system operations, and cleanup
#[test]
#[cfg(all(feature = "agent", feature = "sanity"))]
fn test_agent_create_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /agent create --name <agent_name> command... | Description: Tests the <code> /agent create</code> command to create a new agent with specified name. Verifies agent creation process, file system operations, and cleanup");
    
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let agent_name = format!("test_demo_agent_{}", timestamp);
    
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let create_response = chat.execute_command(&format!("/agent create --name {}", agent_name))?;
    
    println!("📝 Agent create response: {} bytes", create_response.len());
    println!("📝 CREATE RESPONSE:");
    println!("{}", create_response);
    println!("📝 END CREATE RESPONSE");
    
    let save_response = chat.execute_command(":wq")?;
    
    println!("📝 Save response: {} bytes", save_response.len());
    println!("📝 SAVE RESPONSE:");
    println!("{}", save_response);
    println!("📝 END SAVE RESPONSE");
    
    assert!(save_response.contains("Agent") && save_response.contains(&agent_name) && save_response.contains("has been created successfully"), "Missing agent creation success message");
    println!("✅ Found agent creation success message");
    
    let whoami_response = chat.execute_command("!whoami")?;
    
    println!("📝 Whoami response: {} bytes", whoami_response.len());
    println!("📝 WHOAMI RESPONSE:");
    println!("{}", whoami_response);
    println!("📝 END WHOAMI RESPONSE");
    
    let lines: Vec<&str> = whoami_response.lines().collect();
    let username = lines.iter()
        .find(|line| !line.starts_with("!") && !line.starts_with(">") && !line.trim().is_empty())
        .expect("Failed to get username from whoami command")
        .trim();
    println!("✅ Current username: {}", username);
    
    let agent_path = format!("/Users/{}/.aws/amazonq/cli-agents/{}.json", username, agent_name);
    println!("✅ Agent path: {}", agent_path);
    
    if std::path::Path::new(&agent_path).exists() {
        std::fs::remove_file(&agent_path)?;
        println!("✅ Agent file deleted: {}", agent_path);
    } else {
        println!("⚠️ Agent file not found at: {}", agent_path);
    }
    
    assert!(!std::path::Path::new(&agent_path).exists(), "Agent file should be deleted");
    println!("✅ Agent deletion verified");
    
    // Release the lock before cleanup
    drop(chat);
 
    Ok(())
}

/// Tests the /agent create command without required arguments to verify error handling
/// Verifies proper error messages, usage information, and help suggestions
#[test]
#[cfg(all(feature = "agent", feature = "sanity"))]
fn test_agent_create_missing_args() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /agent create without required arguments... | Description: Tests the <code> /agent create</code> command without required arguments to verify error handling. Verifies proper error messages, usage information, and help suggestions");
    
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("/agent create")?;
    
    println!("📝 Agent create missing args response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    assert!(response.contains("error:"), "Missing error message part 1a");
    assert!(response.contains("the following required arguments"), "Missing error message part 1b");
    assert!(response.contains("were not provided:"), "Missing error message part 2");
    assert!(response.contains("--name"), "Missing required name argument part 1");
    assert!(response.contains("<NAME>"), "Missing required name argument part 2");
    println!("✅ Found error message for missing required arguments");
    
    assert!(response.contains("Usage:"), "Missing usage information part 1");
    assert!(response.contains("/agent create"), "Missing usage information part 2a");
    assert!(response.contains("--name <NAME>"), "Missing usage information part 2b");
    println!("✅ Found usage information");
    
    assert!(response.contains("For more information"), "Missing help suggestion part 1");
    assert!(response.contains("try"), "Missing help suggestion part 2a");
    println!("✅ Found help suggestion");
    
    assert!(response.contains("Options:"), "Missing options section");
    assert!(response.contains("<NAME>"), "Missing name option part 2");
    assert!(response.contains("Name of the agent to be created"), "Missing name description");
    assert!(response.contains("<DIRECTORY>"), "Missing directory option part 2");
    assert!(response.contains("<FROM>"), "Missing from option part 2");
    println!("✅ Found all expected options");
    
    println!("✅ /agent create executed successfully with expected error for missing arguments");
    
    // Release the lock before cleanup
    drop(chat);
       
    Ok(())
}

/// Tests the /agent help command to display comprehensive agent help information
/// Verifies agent descriptions, usage notes, launch instructions, and configuration paths
#[test]
#[cfg(all(feature = "agent", feature = "sanity"))]
fn test_agent_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /agent help... | Description: Tests the <code> /agent help</code> command to display comprehensive agent help information. Verifies agent descriptions, usage notes, launch instructions, and configuration paths");
    
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("/agent help")?;
    
    println!("📝 Agent help command response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    assert!(response.contains("~/.aws/amazonq/cli-agents/"), "Missing global path");
    assert!(response.contains("cwd/.amazonq/cli-agents"), "Missing workspace path");
    assert!(response.contains("Usage:"), "Missing usage label");
    assert!(response.contains("/agent"), "Missing agent command");
    assert!(response.contains("<COMMAND>"), "Missing command parameter");
    assert!(response.contains("Commands:"), "Missing commands section");
    assert!(response.contains("list"), "Missing list command");
    assert!(response.contains("create"), "Missing create command");
    assert!(response.contains("schema"), "Missing schema command");
    assert!(response.contains("set-default"), "Missing set-default command");
    assert!(response.contains("help"), "Missing help command");
    println!("✅ Found all expected commands in help output");
    
    assert!(response.contains("Options:"), "Missing options section");
    assert!(response.contains("-h"), "Missing short help flag");
    assert!(response.contains("--help"), "Missing long help flag");
    println!("✅ Found all expected options in help output");
    
    println!("✅ All expected help content found");
    println!("✅ /agent help executed successfully");
    
    // Release the lock before cleanup
    drop(chat);

    Ok(())
}

/// Tests the /agent command with invalid subcommand to verify error handling
/// Verifies that invalid commands display help information with available commands and options
#[test]
#[cfg(all(feature = "agent", feature = "sanity"))]
fn test_agent_invalid_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /agent invalidcommand... | Description: Tests the <code> /agent</code> command with invalid subcommand to verify error handling. Verifies that invalid commands display help information with available commands and options");
    
    let session =q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("/agent invalidcommand")?;
    
    println!("📝 Agent invalid command response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    assert!(response.contains("Commands:"), "Missing commands section");
    assert!(response.contains("list"), "Missing list command");
    assert!(response.contains("create"), "Missing create command");
    assert!(response.contains("schema"), "Missing schema command");
    assert!(response.contains("set-default"), "Missing set-default command");
    assert!(response.contains("help"), "Missing help command");
    println!("✅ Found all expected commands in help output");
    
    assert!(response.contains("Options:"), "Missing options section");
    println!("✅ Found options section");
    
    println!("✅ /agent invalidcommand executed successfully with expected error");
    
    // Release the lock before cleanup
    drop(chat);
      
    Ok(())
}

/// Tests the /agent list command to display all available agents
/// Verifies agent listing format and presence of default agent
#[test]
#[cfg(all(feature = "agent", feature = "sanity"))]
fn test_agent_list_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /agent list command... | Description: Tests the <code> /agent list</code> command to display all available agents. Verifies agent listing format and presence of default agent");
    
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("/agent list")?;
    
    println!("📝 Agent list response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    assert!(response.contains("q_cli_default"), "Missing q_cli_default agent");
    println!("✅ Found q_cli_default agent in list");
    
    assert!(response.contains("* q_cli_default"), "Missing bullet point format for q_cli_default");
    println!("✅ Verified bullet point format for agent list");
    
    println!("✅ /agent list command executed successfully");
    
    // Release the lock before cleanup
    drop(chat);
   
    Ok(())
}

/// Tests the /agent schema command to display agent configuration schema
/// Verifies JSON schema structure with required keys and properties
// #[test]
// #[cfg(feature = "agent")]
// fn test_agent_schema_command() -> Result<(), Box<dyn std::error::Error>> {
//     println!("\n🔍 Testing /agent schema... | Description: Tests the /agent schema command to display agent configuration schema. Verifies JSON schema structure with required keys and properties");
    
//     let session = get_chat_session();
//     let mut chat = session.lock().unwrap();
    
//     let response = chat.execute_command("/agent schema")?;
    
//     println!("📝 Agent schema response: {} bytes", response.len());
//     println!("📝 FULL OUTPUT:");
//     println!("{}", response);
//     println!("📝 END OUTPUT");
    
//     let mut failures = Vec::new();
    
//     if !response.contains("$schema") { failures.push("Missing $schema key"); }
//     if !response.contains("title") { failures.push("Missing title key"); }
//     if !response.contains("description") { failures.push("Missing description key"); }
//     if !response.contains("type") { failures.push("Missing type key"); }
//     if !response.contains("properties") { failures.push("Missing properties key"); }
    
//     if !failures.is_empty() {
//         panic!("Test failures: {}", failures.join(", "));
//     }
    
//     println!("✅ Found all expected JSON schema keys and properties");
//     println!("✅ /agent schema executed successfully with valid JSON schema");
    
//     // Release the lock before cleanup
//     drop(chat);
     
//     Ok(())
// }

/// Tests the /agent set-default command with valid arguments to set default agent
/// Verifies success messages and confirmation of default agent configuration
#[test]
#[cfg(all(feature = "agent", feature = "sanity"))]
fn test_agent_set_default_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /agent set-default with valid arguments... | Description: Tests the <code> /agent set-default</code> command with valid arguments to set default agent. Verifies success messages and confirmation of default agent configuration");
    
    let session =q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("/agent set-default -n q_cli_default")?;
    
    println!("📝 Agent set-default command response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    let mut failures = Vec::new();
    
    if !response.contains("✓") { failures.push("Missing success checkmark"); }
    if !response.contains("Default agent set to") { failures.push("Missing success message"); }
    if !response.contains("q_cli_default") { failures.push("Missing agent name"); }
    if !response.contains("This will take effect") { failures.push("Missing effect message"); }
    if !response.contains("next time q chat is launched") { failures.push("Missing launch message"); }
    
    if !failures.is_empty() {
        panic!("Test failures: {}", failures.join(", "));
    }
    
    println!("✅ All expected success messages found");
    println!("✅ /agent set-default executed successfully with valid arguments");
    
    // Release the lock before cleanup
    drop(chat);

    
    Ok(())
}

/// Tests the /agent set-default command without required arguments to verify error handling
/// Verifies error messages, usage information, and available options display
#[test]
#[cfg(all(feature = "agent", feature = "sanity"))]
fn test_agent_set_default_missing_args() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /agent set-default without required arguments... | Description: Tests the <code> /agent set-default</code> command without required arguments to verify error handling. Verifies error messages, usage information, and available options display");
    
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("/agent set-default")?;
    
    println!("📝 Agent set-default missing args response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    let mut failures = Vec::new();
    
    if !response.contains("error") { failures.push("Missing error message"); }
    if !response.contains("the following required arguments were not provided:") { failures.push("Missing error message2"); }
    if !response.contains("--name <NAME>") { failures.push("Missing required name argument"); }
    if !response.contains("Usage:") { failures.push("Missing usage text"); }
    if !response.contains("/agent") { failures.push("Missing agent command"); }
    if !response.contains("set-default") { failures.push("Missing set-default subcommand"); }
    if !response.contains("--name") { failures.push("Missing name flag"); }
    if !response.contains("For more information") { failures.push("Missing help text"); }
    if !response.contains("--help") { failures.push("Missing help flag"); }
    if !response.contains("Options:") { failures.push("Missing options section"); }
    if !response.contains("-n") { failures.push("Missing short name flag"); }
    if !response.contains("<NAME>") { failures.push("Missing name parameter"); }
    if !response.contains("-h") { failures.push("Missing short help flag"); }
    if !response.contains("Print help") { failures.push("Missing help description"); }
    
    if !failures.is_empty() {
        panic!("Test failures: {}", failures.join(", "));
    }
    
    println!("✅ All expected error messages and options found");
    println!("✅ /agent set-default executed successfully with expected error for missing arguments");
    
    // Release the lock before cleanup
    drop(chat);
   
    Ok(())
}