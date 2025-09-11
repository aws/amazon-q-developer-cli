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
    "test_compact_command",
    "test_compact_help_command",
    "test_compact_h_command",
    "test_compact_truncate_true_command",
    "test_compact_truncate_false_command",
    "test_show_summary",
    "test_max_message_truncate_true",
    "test_max_message_truncate_false",
    "test_max_message_length_invalid",
    "test_compact_messages_to_exclude_command",
    "test_compact_messages_to_exclude_show_sumary_command",
];
#[allow(dead_code)]
const TOTAL_TESTS: usize = TEST_NAMES.len();

#[test]
#[cfg(all(feature = "compact", feature = "sanity"))]
fn test_compact_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /compact command... | Description: Tests the <code>/compact</code> command to compress conversation history and verify successful compaction or appropriate messaging for short conversations");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
     
    let response = chat.execute_command("What is AWS?")?;
    
    println!("📝 AI response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

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
#[cfg(all(feature = "compact", feature = "sanity"))]
fn test_compact_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /compact --help command... | Description: Tests the <code> /compact --help</code> command to display comprehensive help information for conversation compaction functionality");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let response = chat.execute_command("/compact --help")?;
    
    println!("📝 Compact help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
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
    assert!(response.contains("-h") &&  response.contains("--help"), "Missing -h, --help flags");
    println!("✅ Found all options and help flags");
    
    println!("✅ All compact help content verified!");
    
     // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "compact", feature = "sanity"))]
fn test_compact_h_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /compact -h command... | Description: Tests the <code> /compact -h</code> command (short form) to display compact help information");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let response = chat.execute_command("/compact -h")?;
    
    println!("📝 Compact help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
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
    assert!(response.contains("-h") &&  response.contains("--help"), "Missing -h, --help flags");
    println!("✅ Found all options and help flags");
    
    println!("✅ All compact help content verified!");
    
     // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "compact", feature = "sanity"))]
fn test_compact_truncate_true_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /compact --truncate-large-messages true command... | Description: Test that the <code> /compact  —truncate-large-messages true</code> truncates large messages");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
     
    let response = chat.execute_command("What is AWS?")?;
    
    println!("📝 AI response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    let response = chat.execute_command("/compact --truncate-large-messages true")?;
    
    println!("📝 Compact response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    if response.to_lowercase().contains("truncating") {
        println!("✅ Truncation of large messages verified!");
        if response.contains("history") && response.contains("compacted") && response.contains("successfully") {
            println!("✅ Found compact success message");
        }
    } else if response.contains("Conversation") && response.contains("short") {
        println!("✅ Found conversation too short message");
    } else {
        panic!("Missing expected message");
    }
    
    println!("✅ All compact content verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "compact", feature = "sanity"))]
fn test_compact_truncate_false_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /compact --truncate-large-messages false command... | Description: Tests the <code> /compact --truncate-large-messages false</code> command to verify no message truncation occurs");
    
    let session = get_chat_session();
     let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
     
    let response = chat.execute_command("What is AWS?")?;
    
    println!("📝 AI response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    let response = chat.execute_command("/compact --truncate-large-messages false")?;
    
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
#[cfg(all(feature = "compact", feature = "sanity"))]
fn test_show_summary() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /compact --show-summary command... | Description: Tests the <code> /compact --show-summary</code> command to display conversation summary after compaction");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let response = chat.execute_command("What is AWS?")?;
    
    println!("📝 AI response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    let response = chat.execute_command("What is DL?")?;
    
    println!("📝 AI response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    let response = chat.execute_command("/compact --show-summary")?;
    
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
    
    // Verify compact sumary response
    assert!(response.to_lowercase().contains("conversation") && response.to_lowercase().contains("summary"), "Missing Summary section");
    println!("✅ All compact content verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "compact", feature = "sanity"))]
fn test_max_message_truncate_true() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /compact --truncate-large-messages true --max-message-length command... | Description: Test <code> /compact --truncate-large-messages true  --max-message-length <MAX_MESSAGE_LENGTH></code> command compacts the conversation by summarizing it to free up context space, truncating large messages to a maximum of provided <MAX_MESSAGE_LENGTH>. ");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let response = chat.execute_command("What is AWS?")?;
    
    println!("📝 AI response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    let response = chat.execute_command("What is DL?")?;
    
    println!("📝 AI response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    let response = chat.execute_command("/compact --truncate-large-messages true  --max-message-length 5")?;
    
    println!("📝 Compact response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify compact response - either success or too short
    if response.to_lowercase().contains("truncating") {
        println!("✅ Truncation of large messages verified!");
        if response.contains("history") && response.contains("compacted") && response.contains("successfully") {
            println!("✅ Found compact success message");
        }
    } else if response.contains("Conversation") && response.contains("short") {
        println!("✅ Found conversation too short message");
    } else {
        panic!("Missing expected message");
    }
    
    // Verify compact sumary response
    assert!(response.to_lowercase().contains("conversation") && response.to_lowercase().contains("summary"), "Missing Summary section");
    println!("✅ All compact content verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "compact", feature = "sanity"))]
fn test_max_message_truncate_false() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /compact --truncate-large-messages false --max-message-length command... | Description: Test <code> /compact --truncate-large-messages false --max-message-length <MAX_MESSAGE_LENGTH></code> command compacts the conversation by summarizing it to free up context space, but keeps large messages intact (no truncation) despite the max-message-length setting.");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let response = chat.execute_command("What is AWS?")?;
    
    println!("📝 AI response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    let response = chat.execute_command("What is DL?")?;
    
    println!("📝 AI response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    let response = chat.execute_command("/compact --truncate-large-messages false  --max-message-length 5")?;
    
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
    
    // Verify compact sumary response
    assert!(response.to_lowercase().contains("conversation") && response.to_lowercase().contains("summary"), "Missing Summary section");
    println!("✅ All compact content verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "compact", feature = "sanity"))]
fn test_max_message_length_invalid() -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /compact --max-message-length command... | Description: Tests the <code> /compact --max-message-length <MAX_MESSAGE_LENGTH></code> command with invalid subcommand to verify proper error handling and help display");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    
    let response = chat.execute_command("What is AWS?")?;
    
    println!("📝 AI response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    let response = chat.execute_command("What is DL?")?;
    
    println!("📝 AI response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    let response = chat.execute_command("/compact --max-message-length 5")?;
    
    println!("📝 Compact response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify error message for missing required argument
    assert!(response.contains("error"), "Missing error message");
    assert!(response.contains("--truncate-large-messages") && response.contains("<TRUNCATE_LARGE_MESSAGES>") && response.contains("--max-message-length") && response.contains("<MAX_MESSAGE_LENGTH>"), "Missing required argument info");
    assert!(response.contains("Usage"), "Missing usage info");
    assert!(response.contains("--help"), "Missing help suggestion");
    println!("✅ Found expected error message for missing --truncate-large-messages argument");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "compact", feature = "sanity"))]
fn test_compact_messages_to_exclude_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /compact command... | Description: Test <code> /compact --messages-to-exclude <MESSAGES_TO_EXCLUDE></code> command compacts the conversation by summarizing it to free up context space, excluding provided number of user-assistant message pair from the summarization process.");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
     
    let response = chat.execute_command("What is AWS?")?;
    
    println!("📝 AI response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    let response = chat.execute_command("What is fibonacci?")?;
    
    println!("📝 AI response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    let response = chat.execute_command("/compact --messages-to-exclude 1")?;
    
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
#[cfg(all(feature = "compact", feature = "sanity"))]
fn test_compact_messages_to_exclude_show_sumary_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /compact command... | Description: Test <code> /compact --messages-to-exclude <MESSAGES_TO_EXCLUDE> --show-summary</code> command compacts the conversation by summarizing it to free up context space, excluding provided number of user-assistant message pair from the summarization process and prints the coversation summary.");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let response = chat.execute_command("/clear")?;
    let response = chat.execute_command("y")?;
    let response = chat.execute_command("What is AWS?")?;
    
    println!("📝 AI response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    let response = chat.execute_command("What is fibonacci?")?;
    
    println!("📝 AI response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    let response = chat.execute_command("/compact --messages-to-exclude 1 --show-summary")?;
    
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
    
    // Verify compact sumary response
    assert!(response.to_lowercase().contains("conversation") && response.to_lowercase().contains("summary"), "Missing Summary section");
    println!("✅ All compact content verified!");

    // Verify messages got excluded
    assert!(!response.to_lowercase().contains("fibonacci"), "Fibonacci should not be present in compact response");
    println!("✅ All compact content verified!");

    println!("✅ All compact content verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}