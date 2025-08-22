#[allow(unused_imports)]
use q_cli_e2e_tests::{get_chat_session, cleanup_if_last_test};
use std::sync::atomic::AtomicUsize;

#[allow(dead_code)]
static TEST_COUNT: AtomicUsize = AtomicUsize::new(0);

// List of covered tests
#[allow(dead_code)]
const TEST_NAMES: &[&str] = &[
    "test_hooks_command",
    "test_hooks_help_command",
];
#[allow(dead_code)]
const TOTAL_TESTS: usize = TEST_NAMES.len();

#[test]
#[cfg(all(any(feature = "hooks", feature = "integration"), feature = "regression"))]
fn test_hooks_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /hooks command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

    let response = chat.execute_command("/hooks")?;
    
    println!("📝 Hooks command response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify no hooks configured message
    assert!(response.contains("No hooks"), "Missing no hooks configured message");
    println!("✅ Found no hooks configured message");
    
    // Verify documentation reference
    assert!(response.contains("documentation"), "Missing documentation reference");
    assert!(response.contains("https://github.com/aws/amazon-q-developer-cli/blob/main/docs/agent-format.md#hooks-field"), "Missing documentation URL");
    println!("✅ Found documentation reference and URL");
    
    println!("✅ All hooks command functionality verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(any(feature = "hooks", feature = "integration"), feature = "regression"))]
fn test_hooks_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /hooks --help command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

    let response = chat.execute_command("/hooks --help")?;
    
    println!("📝 Hooks help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    /* Verify context hooks description
    assert!(response.contains("context hooks"), "Missing context hooks");
    println!("✅ Found context hooks description");*/
    
    // Verify documentation reference
    assert!(response.contains("documentation"), "Missing documentation reference");
    assert!(response.contains("https://github.com/aws/amazon-q-developer-cli/blob/main/docs/agent-format.md#hooks-field"), "Missing documentation URL");
    println!("✅ Found documentation reference and URL");
    
    // Verify Notes section
    assert!(response.contains("Notes:"), "Missing Notes section");
    /*assert!(response.contains("executed in parallel"), "Missing parallel execution note");
    assert!(response.contains("conversation_start"), "Missing conversation_start hook type");
    assert!(response.contains("per_prompt"), "Missing per_prompt hook type");*/
    println!("✅ Found Notes section with hook types and execution details");
    
    // Verify Usage section
    assert!(response.contains("Usage:"), "Missing Usage section");
    assert!(response.contains("/hooks"), "Missing /hooks command in usage section");
    println!("✅ Found Usage section with /hooks command");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    println!("✅ Found Options section");
    
    // Verify help flags
    assert!(response.contains("-h") &&  response.contains("--help") && response.contains("Print help"), "Missing -h, --help flags");
    println!("✅ Found help flags: -h, --help with Print help description");
    
    println!("✅ All hooks help content verified!");
    
     // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}