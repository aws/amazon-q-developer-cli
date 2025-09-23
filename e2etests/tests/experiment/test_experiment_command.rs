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

#[allow(dead_code)]
const TEST_NAMES: &[&str] = &[
    "test_knowledge_command",
    "test_thinking_command", 
    "test_experiment_help_command",
];
#[allow(dead_code)]
const TOTAL_TESTS: usize = TEST_NAMES.len();

#[test]
#[cfg(all(feature = "experiment", feature = "sanity"))]
fn test_knowledge_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /experiment command... | Description: Tests the <code>  /experiment </code> command to toggle Knowledge experimental features");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/experiment")?;
    
    println!("📝 Experiment response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify experiment menu content
    assert!(response.contains("Select"), "Missing selection prompt");
    assert!(response.contains("Knowledge"), "Missing Knowledge experiment");
    println!("✅ Found experiment menu with Knowledge option");
    
    // Find Knowledge and check if it's already selected
    let lines: Vec<&str> = response.lines().collect();
    let mut knowledge_menu_position = 0;
    let mut knowledge_state = false;
    let mut found = false;
    let mut knowledge_already_selected = false;
    
    // Check if Knowledge is already selected (has ❯)
    for line in lines.iter() {
        if line.contains("Knowledge") && line.trim_start().starts_with("❯") {
            knowledge_already_selected = true;
            knowledge_state = line.contains("[ON]");
            found = true;
            break;
        }
    }
    
    // If not selected, find its position
    if !knowledge_already_selected {
        let mut menu_position = 0;
        for line in lines.iter() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("❯") || (trimmed.contains("[ON]") || trimmed.contains("[OFF]")) {
                if line.contains("Knowledge") {
                    knowledge_menu_position = menu_position;
                    knowledge_state = line.contains("[ON]");
                    found = true;
                    break;
                }
                menu_position += 1;
            }
        }
    }
    
    assert!(found, "Knowledge option not found in menu");
    println!("📝 Knowledge already selected: {}, position: {}, state: {}", knowledge_already_selected, knowledge_menu_position, if knowledge_state { "ON" } else { "OFF" });
    
    // Navigate to Knowledge option using arrow keys (only if not already selected)
    if !knowledge_already_selected {
        for _ in 0..knowledge_menu_position {
            chat.send_key_input("\x1b[B")?; // Down arrow
        }
    }
    
    // Select the Knowledge option
    let navigate_response = chat.send_key_input("\r")?; // Enter
    
    println!("📝 Navigate response: {} bytes", navigate_response.len());
    println!("📝 NAVIGATE RESPONSE:");
    println!("{}", navigate_response);
    println!("📝 END NAVIGATE RESPONSE");
    
    // Verify toggle response based on previous state
    if knowledge_state {
        assert!(navigate_response.contains("Knowledge experiment disabled"), "Expected Knowledge to be disabled");
        println!("✅ Knowledge experiment disabled successfully");
    } else {
        assert!(navigate_response.contains("Knowledge experiment enabled"), "Expected Knowledge to be enabled");
        println!("✅ Knowledge experiment enabled successfully");
    }
    
    // Test reverting back to original state (run command again)
    println!("📝 Testing revert to original state...");
    let revert_response = chat.execute_command("/experiment")?;
    
    // Navigate to Knowledge option again (only if not already selected)
    if !knowledge_already_selected {
        for _ in 0..knowledge_menu_position {
            chat.send_key_input("\x1b[B")?; // Down arrow
        }
    }
    let revert_navigate_response = chat.send_key_input("\r")?; // Enter
    
    println!("📝 Revert response: {} bytes", revert_navigate_response.len());
    println!("📝 REVERT RESPONSE:");
    println!("{}", revert_navigate_response);
    println!("📝 END REVERT RESPONSE");
    
    // Verify it reverted to original state
    if knowledge_state {
        assert!(revert_navigate_response.contains("Knowledge experiment enabled"), "Expected Knowledge to be enabled (reverted)");
        println!("✅ Knowledge experiment reverted to enabled successfully");
    } else {
        assert!(revert_navigate_response.contains("Knowledge experiment disabled"), "Expected Knowledge to be disabled (reverted)");
        println!("✅ Knowledge experiment reverted to disabled successfully");
    }

    println!("✅ /experiment command test completed successfully");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(all(feature = "experiment", feature = "sanity"))]
fn test_thinking_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /experiment command... | Description: Tests the <code>  /experiment </code> command to toggle thinking experimental features");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/experiment")?;
    
    println!("📝 Experiment response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify experiment menu content
    assert!(response.contains("Select"), "Missing selection prompt");
    assert!(response.contains("Thinking"), "Missing Thinking experiment");
    println!("✅ Found experiment menu with Thinking option");
    
    // Find Thinking and check if it's already selected
    let lines: Vec<&str> = response.lines().collect();
    let mut Thinking_menu_position = 0;
    let mut Thinking_state = false;
    let mut found = false;
    let mut Thinking_already_selected = false;
    
    // Check if Thinking is already selected (has ❯)
    for line in lines.iter() {
        if line.contains("Thinking") && line.trim_start().starts_with("❯") {
            Thinking_already_selected = true;
            Thinking_state = line.contains("[ON]");
            found = true;
            break;
        }
    }
    
    // If not selected, find its position
    if !Thinking_already_selected {
        let mut menu_position = 0;
        for line in lines.iter() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("❯") || (trimmed.contains("[ON]") || trimmed.contains("[OFF]")) {
                if line.contains("Thinking") {
                    Thinking_menu_position = menu_position;
                    Thinking_state = line.contains("[ON]");
                    found = true;
                    break;
                }
                menu_position += 1;
            }
        }
    }
    
    assert!(found, "Thinking option not found in menu");
    println!("📝 Thinking already selected: {}, position: {}, state: {}", Thinking_already_selected, Thinking_menu_position, if Thinking_state { "ON" } else { "OFF" });
    
    // Navigate to Thinking option using arrow keys (only if not already selected)
    if !Thinking_already_selected {
        for _ in 0..Thinking_menu_position {
            chat.send_key_input("\x1b[B")?; // Down arrow
        }
    }
    
    // Select the Thinking option
    let navigate_response = chat.send_key_input("\r")?; // Enter
    
    println!("📝 Navigate response: {} bytes", navigate_response.len());
    println!("📝 NAVIGATE RESPONSE:");
    println!("{}", navigate_response);
    println!("📝 END NAVIGATE RESPONSE");
    
    // Verify toggle response based on previous state
    if Thinking_state {
        assert!(navigate_response.contains("Thinking experiment disabled"), "Expected Thinking to be disabled");
        println!("✅ Thinking experiment disabled successfully");
    } else {
        assert!(navigate_response.contains("Thinking experiment enabled"), "Expected Thinking to be enabled");
        println!("✅ Thinking experiment enabled successfully");
    }
    
    // Test reverting back to original state (run command again)
    println!("📝 Testing revert to original state...");
    let revert_response = chat.execute_command("/experiment")?;
    
    // Navigate to Thinking option again (only if not already selected)
    if !Thinking_already_selected {
        for _ in 0..Thinking_menu_position {
            chat.send_key_input("\x1b[B")?; // Down arrow
        }
    }
    let revert_navigate_response = chat.send_key_input("\r")?; // Enter
    
    println!("📝 Revert response: {} bytes", revert_navigate_response.len());
    println!("📝 REVERT RESPONSE:");
    println!("{}", revert_navigate_response);
    println!("📝 END REVERT RESPONSE");
    
    // Verify it reverted to original state
    if Thinking_state {
        assert!(revert_navigate_response.contains("Thinking experiment enabled"), "Expected Thinking to be enabled (reverted)");
        println!("✅ Thinking experiment reverted to enabled successfully");
    } else {
        assert!(revert_navigate_response.contains("Thinking experiment disabled"), "Expected Thinking to be disabled (reverted)");
        println!("✅ Thinking experiment reverted to disabled successfully");
    }

    println!("✅ /experiment command test completed successfully");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(all(feature = "experiment", feature = "sanity"))]
fn test_experiment_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /experiment --help command... | Description: Tests the <code> /experiment --help</code> command to display help information");

    let session = get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/experiment --help")?;
    
    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify help content
    assert!(response.contains("Usage:") && response.contains("/experiment"),  "Missing usage information");
    assert!(response.contains("Options:"), "Missing Options section");
    assert!(response.contains("-h") &&  response.contains("--help"), "Missing -h, --help flags");
    println!("✅ Found all expected help content");

    println!("✅ /experiment --help command test completed successfully");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}