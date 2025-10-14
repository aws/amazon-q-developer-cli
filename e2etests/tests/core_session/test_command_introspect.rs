#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;

//Test the introspect command
#[test]
#[cfg(all(feature = "core_session", feature = "sanity"))]
fn test_introspect_command() -> Result<(), Box<dyn std::error::Error>> {

    println!("\n🔍 Testing introspect command... | Description: Tests the introspect command.");

    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("introspect")?;
    println!("📝 Help response: {} bytes", response);
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Basic validation - check for key elements
    assert!(!response.is_empty(), "Expected non-empty response");
    assert!(response.contains("Amazon Q"), "Missing Amazon Q identification");
    assert!(response.contains("assistant") || response.contains("AI"), "Missing AI assistant reference");
    assert!(response.contains("/quit") || response.contains("quit"), "Missing quit command");
    
    println!("✅ Introspect command executed successfully");

    // Release the lock
    drop(chat);
    
    Ok(())
}