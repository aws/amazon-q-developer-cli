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
    "test_model_dynamic_command",
    "test_model_help_command",
    "test_model_h_command",
];
#[allow(dead_code)]
const TOTAL_TESTS: usize = TEST_NAMES.len();

#[test]
#[cfg(all(feature = "model", feature = "sanity"))]
fn test_model_dynamic_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /model command with dynamic selection... | Description: Tests the /model command interactive selection interface to choose different models and verify selection confirmation");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

    // Execute /model command to get list
    let model_response = chat.execute_command("/model")?;
    
    println!("📝 Model response: {} bytes", model_response.len());
    println!("📝 MODEL RESPONSE:");
    println!("{}", model_response);
    println!("📝 END MODEL RESPONSE");
    
    // Helper function to strip ANSI color codes
    let strip_ansi = |s: &str| -> String {
        let mut result = String::new();
        let mut in_escape = false;
        for c in s.chars() {
            if c == '\x1b' {
                in_escape = true;
            } else if in_escape && c == 'm' {
                in_escape = false;
            } else if !in_escape {
                result.push(c);
            }
        }
        result
    };
    
    // Parse available models from response
    let mut models = Vec::new();
    let mut found_prompt = false;
    
    for line in model_response.lines() {
        let trimmed_line = line.trim();
        
        // Look for the prompt line
        if trimmed_line.contains("Select a model for this chat session") {
            found_prompt = true;
            continue;
        }
        
        // After finding prompt, parse model lines
        if found_prompt {
            let cleaned_line = strip_ansi(trimmed_line);
            println!("\n🔍 Row: '{}' -> Cleaned: '{}'", trimmed_line, cleaned_line);
            
            if !trimmed_line.is_empty() {
                // Check if line contains a model (starts with ❯, spaces, or contains model names)
                if cleaned_line.starts_with("❯") || cleaned_line.starts_with(" ") || cleaned_line.contains("-") {
                    let model_name = cleaned_line
                        .replace("❯", "")
                        .replace("(active)", "")
                        .trim()
                        .to_string();
                    
                    println!("\n🔍 Extracted model: '{}'", model_name);
                    if !model_name.is_empty() {
                        models.push(model_name);
                    }
                }
            }
        }
    }
    
    println!("📝 Found models: {:?}", models);
    assert!(!models.is_empty(), "No models found in response");
    
    // Send down arrow to select different model
    let selection_response = chat.send_key_input("\x1b[B")?;
    
    println!("📝 Selection response: {} bytes", selection_response.len());
    println!("📝 SELECTION RESPONSE:");
    println!("{}", selection_response);
    println!("📝 END SELECTION RESPONSE");
    
    // Find which model is now selected (has ❯ marker)
    let selected_model = selection_response.lines()
        .find(|line| {
            let cleaned = strip_ansi(line);
            cleaned.contains("❯")
        })
        .map(|line| {
            let cleaned = strip_ansi(line.trim());
            cleaned
                .replace("❯", "")
                .replace("(active)", "")
                .trim()
                .to_string()
        })
        .unwrap_or_else(|| models.get(1).unwrap_or(&models[0]).clone());
    
    println!("📝 Selected model: {}", selected_model);
    
    // Send Enter to confirm
    let confirm_response = chat.send_key_input("\r")?;
    
    println!("📝 Confirm response: {} bytes", confirm_response.len());
    println!("📝 CONFIRM RESPONSE:");
    println!("{}", confirm_response);
    println!("📝 END CONFIRM RESPONSE");
    
    // Verify selection with dynamic model name
    assert!(confirm_response.contains(&format!("Using {}", selected_model)), 
           "Missing confirmation for selected model: {}", selected_model);
    println!("✅ Confirmed selection of: {}", selected_model);
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "model", feature = "sanity"))]
fn test_model_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /model --help command... | Description: Tests the /model --help command to display help information for model selection functionality");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

    let response = chat.execute_command("/model --help")?;
    
    println!("📝 Model help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify Usage section
    assert!(response.contains("Usage:"), "Missing Usage section");
    assert!(response.contains("/model"), "Missing /model command in usage section");
    println!("✅ Found Usage section with /model command");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    println!("✅ Found Options section");
    
    // Verify help flags
    assert!(response.contains("-h") &&  response.contains("--help"), "Missing -h, --help flags");
    println!("✅ Found help flags: -h, --help with Print help description");
    
    println!("✅ All model help content verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}

#[test]
#[cfg(all(feature = "model", feature = "sanity"))]
fn test_model_h_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /model -h command... | Description: Tests the /model -h command (short form) to display help information for model selection functionality");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();

    let response = chat.execute_command("/model -h")?;
    
    println!("📝 Model help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Verify Usage section
    assert!(response.contains("Usage:"), "Missing Usage section");
    assert!(response.contains("/model"), "Missing /model command in usage section");
    println!("✅ Found Usage section with /model command");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    println!("✅ Found Options section");
    
    // Verify help flags
    assert!(response.contains("-h") &&  response.contains("--help"), "Missing -h, --help flags");
    println!("✅ Found help flags: -h, --help with Print help description");
    
    println!("✅ All model help content verified!");
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;

    Ok(())
}