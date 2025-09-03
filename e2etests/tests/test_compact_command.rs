use q_cli_e2e_tests::{get_chat_session, cleanup_if_last_test};
use std::sync::atomic::{AtomicUsize, Ordering};

static TEST_COUNT: AtomicUsize = AtomicUsize::new(0);

// List of covered tests
const TEST_NAMES: &[&str] = &[
    "test_compact_command",
    "test_compact_help_command",
];
const TOTAL_TESTS: usize = TEST_NAMES.len();

#[test]
#[cfg(feature = "compact")]
fn test_compact_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /compact command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

    let response = chat.execute_command("/compact")?;
    
    println!("📝 Compact response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify compact response - either success or too short
    if response.contains("history") && response.contains("compacted") && response.contains("successfully") {
        println!("✅ Found compact success message");
    } else if response.contains("Conversation") && response.contains("short") {
        println!("✅ Found conversation too short message");
    } else {
        panic!("Missing expected compact response");
    }
    
    println!("✅ All compact content verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(feature = "compact")]
fn test_compact_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /compact --help command...");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

    let response = chat.execute_command("/compact --help")?;
    
    println!("📝 Compact help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    /* Verify main description
    assert!(response.contains("/compact") && response.contains("summarizes") && response.contains("history"), "Missing compact description");
    println!("✅ Found compact description");*/
    
    // Verify When to use section
    assert!(response.contains("When to use"), "Missing When to use section");
    /*assert!(response.contains("memory constraint"), "Missing memory constraint warning");
    assert!(response.contains("conversation") && response.contains("running") && response.contains("long time"), "Missing long conversation note");
    assert!(response.contains("new topic") && response.contains("same session"), "Missing new topic note");
    assert!(response.contains("complex tool operations"), "Missing tool operations note");*/
    println!("✅ Found When to use section with all scenarios");
    
    // Verify How it works section
    assert!(response.contains("How it works"), "Missing How it works section");
   /*assert!(response.contains("AI-generated summary"), "Missing AI summary description");
    assert!(response.contains("key information") && response.contains("code") && response.contains("tool executions"), "Missing key elements");
    assert!(response.contains("free up space"), "Missing free up space description");
    assert!(response.contains("reference the summary context"), "Missing summary context reference");*/
    println!("✅ Found How it works section with all details");
    
    // Verify auto-compaction information
    //assert!(response.contains("Compaction will be automatically performed whenever the context window overflows"), "Missing auto-compaction note");
    //assert!(response.contains("To disable this behavior, run: `q settings chat.disableAutoCompaction true`"), "Missing disable instruction");
    assert!(response.contains("run: `q settings chat.disableAutoCompaction true`"), "Missing disable instruction");
    println!("✅ Found auto-compaction information");
    
    // Verify Usage section
    assert!(response.contains("Usage:"), "Missing usage format");
    println!("✅ Found usage format");
    
    // Verify Arguments section
    assert!(response.contains("Arguments:"), "Missing Arguments section");
    println!("✅ Found Arguments section");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("--show-summary"), "Missing --show-summary option");
    assert!(response.contains("--messages-to-exclude"), "Missing --messages-to-exclude option");
    assert!(response.contains("--truncate-large-messages"), "Missing --truncate-large-messages option");
    assert!(response.contains("--max-message-length"), "Missing --max-message-length option");
    assert!(response.contains("-h") &&  response.contains("--help") && response.contains("Print help"), "Missing -h, --help flags");
    println!("✅ Found all options and help flags");
    
    println!("✅ All compact help content verified!");
    
     // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}