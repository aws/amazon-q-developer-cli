#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;

#[test]
#[cfg(all(feature = "help", feature = "sanity"))]
fn test_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /help command... | Description: Tests the <code> /help</code> command to display all available commands and verify core functionality like quit, clear, tools, and help commands are present");

    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    println!("✅ Q Chat session started");

    let response = chat.execute_command("/help")?;

    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Verify help content
    assert!(response.contains("Commands:"), "Missing Commands section");
    println!("✅ Found Commands section with all available commands");

    assert!(response.contains("quit"), "Missing quit command");
    assert!(response.contains("clear"), "Missing clear command");
    assert!(response.contains("tools"), "Missing tools command");
    assert!(response.contains("help"), "Missing help command");
    println!("✅ Verified core commands: quit, clear, tools, help");

    // Verify specific useful commands
    if response.contains("context") {
        println!("✅ Found context management command");
    }
    if response.contains("agent") {
        println!("✅ Found agent management command");
    }
    if response.contains("model") {
        println!("✅ Found model selection command");
    }

    println!("✅ All help content verified!");

    // Release the lock
    drop(chat);

    Ok(())
}

#[test]
#[cfg(all(feature = "help", feature = "sanity"))]
fn test_multiline_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing multiline input... | Description: Tests <code>ctrl+J multiline </code>command input with embedded newlines");

    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    println!("✅ Q Chat session started");

    let multiline_input = "what is aws explain in 100 words.\nwhat is AI explain in 100 words";
    let response = chat.send_prompt(multiline_input)?;

    println!("📝 Response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    assert!(response.contains("AWS"), "Response should contain 'AWS'");
    assert!(response.contains("AI"), "Response should contain 'AI'");
    assert!(!response.is_empty(), "Response should not be empty");
    println!("✅ Multiline input processed successfully");

    drop(chat);
    Ok(())
}

#[test]
#[cfg(all(feature = "help", feature = "sanity"))]
fn test_whoami_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing !whoami command... | Description: Tests the <code> !whoami </code> command to display the current user");

    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    println!("✅ Q Chat session started");

    let response = chat.execute_command("!whoami")?;

    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Verify whoami content
    assert!(!response.is_empty(), "Empty response from whoami command");
    println!("✅ Command executed with response");

    // Verify response contains user information
    assert!(response.len() > 0, "Response should contain user information");
    println!("✅ Found user information in response");

    println!("✅ All whoami command functionality verified!");

    // Release the lock
    drop(chat);
    Ok(())
}