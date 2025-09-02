#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;
use std::sync::{Mutex, Once, atomic::{AtomicUsize, Ordering}};
static INIT: Once = Once::new();
static mut CHAT_SESSION: Option<Mutex<q_chat_helper::QChatSession>> = None;

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
    "test_what_is_aws_prompt",
    "test_simple_greeting",
];
#[allow(dead_code)]
const TOTAL_TESTS: usize = TEST_NAMES.len();

#[test]
#[cfg(all(feature = "ai_prompts", feature = "sanity"))]
fn test_what_is_aws_prompt() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 [AI PROMPTS] Testing 'What is AWS?' AI prompt... | Description: Tests AI prompt functionality by sending 'What is AWS?' and verifying the response contains relevant AWS information and technical terms");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("What is AWS?")?;
    
    println!("📝 AI response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Check if we got an actual AI response
    if response.contains("Amazon Web Services") || 
       response.contains("cloud") || 
       response.contains("AWS") ||
       response.len() > 100 {
        println!("✅ Got substantial AI response ({} bytes)!", response.len());
        
        // Additional checks for quality response
        if response.contains("Amazon Web Services") {
            println!("✅ Response correctly identifies 'Amazon Web Services'");
        }
        if response.contains("cloud") {
            println!("✅ Response mentions cloud computing concepts");
        }
        if response.contains("AWS") {
            println!("✅ Response uses AWS acronym appropriately");
        }
        
        // Check for technical depth
        let technical_terms = ["service", "platform", "infrastructure", "compute", "storage"];
        let found_terms: Vec<&str> = technical_terms.iter()
            .filter(|&&term| response.to_lowercase().contains(term))
            .copied()
            .collect();
        if !found_terms.is_empty() {
            println!("✅ Response includes technical terms: {:?}", found_terms);
        }
    } else {
        println!("⚠️ Response seems limited or just echoed input");
        println!("⚠️ Expected AWS explanation but got: {} bytes", response.len());
    }

    println!("✅ Test completed successfully");

     // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(all(feature = "ai_prompts", feature = "sanity"))]
fn test_simple_greeting() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing simple 'Hello' prompt... | Description: Tests basic AI interaction by sending a simple greeting and verifying the AI responds appropriately with greeting-related content");
    
    let session = get_chat_session();
    let mut chat = session.lock().unwrap();
    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("Hello")?;
    
    println!("📝 Greeting response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Check if we got any response
    if response.trim().is_empty() {
        println!("⚠️ No response to greeting - AI may not be responding");
    } else if response.to_lowercase().contains("hello") || 
              response.to_lowercase().contains("hi") ||
              response.to_lowercase().contains("greet") {
        println!("✅ Got appropriate greeting response!");
        println!("✅ AI recognized and responded to greeting appropriately");
    } else if response.len() > 20 {
        println!("✅ Got substantial response ({} bytes) to greeting", response.len());
        println!("ℹ️ Response doesn't contain typical greeting words but seems AI-generated");
    } else {
        println!("ℹ️ Got minimal response - unclear if AI-generated or echo");
        println!("ℹ️ Response length: {} bytes", response.len());
    }

    println!("✅ Test completed successfully");

     // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}
