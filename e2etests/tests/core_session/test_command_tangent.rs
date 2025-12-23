#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;

// Test the tangent command.
#[test]
#[cfg(all(feature = "core_session", feature = "sanity"))]
fn test_tangent_command() -> Result<(), Box<dyn std::error::Error>> {

println!("\n🔍 Testing tangent ... | Description: Tests the <code> /tangent </code> command.");
    let session =q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("/tangent")?;

println!("📝 transform response: {} bytes", response.len());
println!("📝 FULL OUTPUT:");
println!("{}", response);
println!("📝 END OUTPUT");

assert!(!response.is_empty(), "Expected non-empty response");
assert!(response.contains("Created a conversation checkpoint") || response.contains("Restored conversation from checkpoint (↯)")
|| response.contains("Tangent mode is disabled. Enable it with: q settings chat.enableTangentMode true"), "Expected checkpoint message");

 drop(chat);

    
    Ok(())
}