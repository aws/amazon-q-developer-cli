#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;

#[test]
#[cfg(all(feature = "mcp", feature = "sanity"))]
fn test_q_mcp_help_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q mcp --help subcommand... | Description: Tests the <code> q mcp --help</code> subcommand to display comprehensive MCP management help including all commands");
    
    println!("\n🔍 Executing q [subcommand]: 'q mcp --help'");
    let response = q_chat_helper::execute_q_subcommand("q", &["mcp", "--help"])?;
    
    println!("📝 MCP help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify complete help content
    assert!(response.contains("Model Context Protocol (MCP)"), "Missing MCP description");
    assert!(response.contains("Usage") && response.contains("qchat mcp"), "Missing usage information");
    assert!(response.contains("Commands"), "Missing Commands section");
    
    // Verify command descriptions
    assert!(response.contains("add"), "Missing add command description");
    assert!(response.contains("remove"), "Missing remove command description");
    assert!(response.contains("list"), "Missing list command description");
    assert!(response.contains("import"), "Missing import command description");
    assert!(response.contains("status"), "Missing status command description");
    assert!(response.contains("help"), "Missing help command");
    println!("✅ Found all MCP commands with descriptions");
    
    Ok(())
}

#[test]
#[cfg(all(feature = "mcp", feature = "sanity"))]
fn test_q_mcp_remove_help_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q mcp remove --help subcommand... | Description: Tests the <code> q mcp remove --help</code> subcommand to display help information for removing MCP servers");
    
    println!("\n🔍 Executing q [subcommand]: 'q mcp remove --help'");
    let response = q_chat_helper::execute_q_subcommand("q", &["mcp", "remove", "--help"])?;
    
    println!("📝 MCP remove help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify complete help content in final response
    assert!(response.contains("Usage") && response.contains("qchat mcp remove"), "Missing usage information");
    assert!(response.contains("Options"), "Missing option information");
    assert!(response.contains("--name"), "Missing --name option");
    assert!(response.contains("--scope"), "Missing --scope option");
    assert!(response.contains("--agent"), "Missing --agent option");
    assert!(response.contains("-h") && response.contains("--help"), "Missing help option");
    println!("✅ Found all expected MCP remove help content and completion");
    
    Ok(())
}

#[test]
#[cfg(all(feature = "mcp", feature = "sanity"))]
fn test_q_mcp_add_help_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q mcp add --help subcommand... | Description: Tests the <code> q mcp add --help</code> subcommand to display help information for adding new MCP servers");
    
    println!("\n🔍 Executing q [subcommand]: 'q mcp add --help'");
    let response = q_chat_helper::execute_q_subcommand("q", &["mcp", "add", "--help"])?;
    
    println!("📝 Restart response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify mcp add help output
    assert!(response.contains("Usage") && response.contains("qchat mcp add"), "Missing usage information");
    assert!(response.contains("Options"), "Missing Options");
    assert!(response.contains("--name"), "Missing --name option");
    assert!(response.contains("--command"), "Missing --command option");
    assert!(response.contains("--scope"), "Missing --scope option");
    assert!(response.contains("--agent"), "Missing --agent option");
    println!("✅ MCP add help subcommand executed successfully");

    Ok(())
}

#[test]
#[cfg(all(feature = "mcp", feature = "sanity"))]
fn test_q_mcp_import_help_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q mcp import --help subcommand... | Description: Tests the <code> q mcp import --help</code> subcommand to display help information for importing MCP server configurations");
    
    println!("\n🔍 Executing q [subcommand]: 'q mcp import --help'");
    let response = q_chat_helper::execute_q_subcommand("q", &["mcp", "import", "--help"])?;
    
    println!("📝 Restart response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify Options section
    assert!(response.contains("Options"), "Missing Options section");
    assert!(response.contains("--file"), "Missing --file option");
    assert!(response.contains("--force"), "Missing --force option");
    assert!(response.contains("-v") && response.contains("--verbose"), "Missing --verbose option");
    assert!(response.contains("-h") && response.contains("--help"), "Missing --help option");
    println!("✅ Found all options with descriptions");
    
    println!("✅ All q mcp import --help content verified successfully");
    
    Ok(())
}

#[test]
#[cfg(all(feature = "mcp", feature = "sanity"))]
fn test_q_mcp_list_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q mcp list subcommand... | Description: Tests the <code> q mcp list</code> subcommand to display all configured MCP servers and their status");
    
    println!("\n🔍 Executing q [subcommand]: 'q mcp list'");
    let response = q_chat_helper::execute_q_subcommand("q", &["mcp", "list"])?;
    
    println!("📝 MCP list response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify MCP server listing
    assert!(response.contains("q_cli_default"), "Missing q_cli_default server");
    println!("✅ Found MCP server listing with  servers and completion");
    
    Ok(())
}

#[test]
#[cfg(all(feature = "mcp", feature = "sanity"))]
fn test_q_mcp_list_help_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q mcp list --help subcommand... | Description: Tests the <code> q mcp list --help</code> subcommand to display help information for listing MCP servers");
    
    println!("\n🔍 Executing q [subcommand]: 'q mcp list --help'");
    let response = q_chat_helper::execute_q_subcommand("q", &["mcp", "list", "--help"])?;
    
    println!("📝 MCP list help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify help content
    assert!(response.contains("Usage"), "Missing usage format");
    
    // Verify arguments section
    assert!(response.contains("Arguments"), "Missing Arguments section");
    assert!(response.contains("[SCOPE]"), "Missing scope argument");
    
    // Verify options section
    assert!(response.contains("Options"), "Missing Options section");
    assert!(response.contains("-v") && response.contains("--verbose"), "Missing verbose option");
    assert!(response.contains("-h") && response.contains("--help"), "Missing help option");
    
    Ok(())
}

#[test]
#[cfg(all(feature = "mcp", feature = "sanity"))]
fn test_q_mcp_status_help_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q mcp status --help subcommand... | Description: Tests the <code> q mcp status --help</code> subcommand to display help information for checking MCP server status");
    
    // Execute mcp status --help subcommand
    println!("\n🔍 Executing q [subcommand]: 'q mcp status --help'");
    let response = q_chat_helper::execute_q_subcommand("q", &["mcp", "status", "--help"])?;

    println!("📝 Restart response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify usage line
    assert!(response.contains("Usage"), "Missing usage information");
    // Verify Options section
    assert!(response.contains("Options"), "Missing Options section");
    assert!(response.contains("--name"), "Missing --name option");
    assert!(response.contains("-v") && response.contains("--verbose") , "Missing --verbose option");
    assert!(response.contains("-h") && response.contains("--help"), "Missing --help option");
    println!("✅ Found all options with descriptions");
    
    println!("✅ All q mcp status --help content verified successfully");
    
    Ok(())
}

#[test]
#[cfg(all(feature = "mcp", feature = "sanity"))]
fn test_add_and_remove_mcp_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q mcp add and remove subcommands... | Description: Tests the <code> q mcp add</code> and <code> q mcp remove</code> subcommands to add and remove MCP servers");

    // First install uv dependency before starting Q Chat
    println!("\n🔍 Installing uv dependency...");

    std::process::Command::new("pip3")
        .args(["install", "uv", "--break-system-packages"])
        .output()
        .expect("Failed to install uv");
    
    println!("✅ uv dependency installed");

    // First check if MCP already exists using q mcp list
    println!("\n🔍 Checking if aws-documentation MCP already exists...");
    let response = q_chat_helper::execute_q_subcommand("q", &["mcp", "list"])?;

    println!("📝 Response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Check if aws-documentation exists in the list
    if response.contains("aws-documentation") {
        println!("\n🔍 aws-documentation MCP already exists, removing it first...");

        let response = q_chat_helper::execute_q_subcommand("q", &["mcp", "remove", "--name", "aws-documentation"])?;

        println!("📝 Response: {} bytes", response.len());
        println!("📝 FULL OUTPUT:");
        println!("{}", response);
        println!("📝 END OUTPUT");
    
        // Verify successful removal
        assert!(response.contains("Removed") && response.contains("'aws-documentation'"), "Missing removal success message");
        println!("✅ Successfully removed existing aws-documentation MCP");
    } else {
        println!("✅ aws-documentation MCP does not exist, proceeding with add");
    }

    // Now add the MCP server
    println!("\n🔍 Executing q [subcommand]: 'q mcp add --name aws-documentation --command uvx --args awslabs.aws-documentation-mcp-server@latest'");
    let response = q_chat_helper::execute_q_subcommand("q", &["mcp", "add", "--name", "aws-documentation", "--command", "uvx", "--args", "awslabs.aws-documentation-mcp-server@latest"])?;
    
    println!("📝 Response: {} bytes", response.len());
    println!("📝 FULL OUTPUT");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify successful addition
    assert!(response.contains("Added") && response.contains("'aws-documentation'"), "Missing success message");
    assert!(response.contains("/Users/") && response.contains("/.aws/amazonq/mcp.json"), "Missing config file path");
    println!("✅ Found successful addition message");
    
    // Now test removing the MCP server
    println!("\n🔍 Executing q [subcommand]: 'q mcp remove --name aws-documentation'");
    let remove_response = q_chat_helper::execute_q_subcommand("q", &["mcp", "remove", "--name", "aws-documentation"])?;

    println!("📝 Remove response: {} bytes", remove_response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", remove_response);
    println!("📝 END OUTPUT");
    
    // Verify successful removal
    assert!(remove_response.contains("Removed") && remove_response.contains("'aws-documentation'"), "Missing removal success message");
    assert!(remove_response.contains("/Users/") && remove_response.contains("/.aws/amazonq/mcp.json"), "Missing config file path in removal");
    println!("✅ Found successful removal message");

    Ok(())
}

#[test]
#[cfg(all(feature = "mcp", feature = "sanity"))]
fn test_q_mcp_status_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q mcp status --name <server-name> subcommand... | Description: Tests the <code> q mcp status</code> subcommand with server name to display detailed status information for a specific MCP server");

    // First install uv dependency before starting Q Chat
    println!("\n🔍 Installing uv dependency...");

    std::process::Command::new("pip3")
        .args(["install", "uv", "--break-system-packages"])
        .output()
        .expect("Failed to install uv");
    
    println!("✅ uv dependency installed");

    // First check if MCP already exists using q mcp list
    println!("\n🔍 Checking if aws-documentation MCP already exists...");
    let response = q_chat_helper::execute_q_subcommand("q", &["mcp", "list"])?;

    println!("📝 Response: {} bytes", response.len());
    println!("📝 FULL OUTPUT");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Check if aws-documentation exists in the list
    if response.contains("aws-documentation") {
        println!("\n🔍 aws-documentation MCP already exists, removing it first...");

        let response = q_chat_helper::execute_q_subcommand("q", &["mcp", "remove", "--name", "aws-documentation"])?;

        println!("📝 Response: {} bytes", response.len());
        println!("📝 FULL OUTPUT");
        println!("{}", response);
        println!("📝 END OUTPUT");
    
        // Verify successful removal
        assert!(response.contains("Removed") && response.contains("'aws-documentation'"), "Missing removal success message");
        println!("✅ Successfully removed existing aws-documentation MCP");
    } else {
        println!("✅ aws-documentation MCP does not exist, proceeding with add");
    }

    // Execute mcp add command
    println!("\n🔍 Executing q [subcommand]: 'q mcp add --name aws-documentation --command uvx --args awslabs.aws-documentation-mcp-server@latest'");
    let response = q_chat_helper::execute_q_subcommand("q", &["mcp", "add", "--name", "aws-documentation", "--command", "uvx", "--args", "awslabs.aws-documentation-mcp-server@latest"])?;
    
    println!("📝 Response: {} bytes", response.len());
    println!("📝 FULL OUTPUT");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify successful addition
    assert!(response.contains("Added") && response.contains("'aws-documentation'"), "Missing success message");
    println!("✅ Found successful addition message");

    // Allow the tool execution
    let response = q_chat_helper::execute_q_subcommand("q", &["mcp", "status", "--name", "aws-documentation"])?;

    println!("📝 Allow response: {} bytes", response.len());
    println!("📝 FULL OUTPUT");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Verify successful status retrieval
    assert!(response.contains("Scope"), "Missing Scope");
    assert!(response.contains("Agent"), "Missing Agent");
    assert!(response.contains("Command"), "Missing Command");
    assert!(response.contains("Disabled"), "Missing Disabled");
    assert!(response.contains("Env Vars"), "Missing Env Vars");
    
    // Now test removing the MCP server
    println!("\n🔍 Executing q [subcommand]: 'q mcp remove --name aws-documentation'");
    let response = q_chat_helper::execute_q_subcommand("q", &["mcp", "remove", "--name", "aws-documentation"])?;
   
    println!("📝 Remove response: {} bytes", response.len());
    println!("📝 FULL OUTPUT");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify successful removal
    assert!(response.contains("Removed") && response.contains("'aws-documentation'"), "Missing removal success message");
    assert!(response.contains("/Users/") && response.contains("/.aws/amazonq/mcp.json"), "Missing config file path in removal");
    println!("✅ Found successful removal message");
    
    Ok(())
}

