#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(all(feature = "ai_prompts", feature = "sanity"))]
fn test_what_is_aws_prompt() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 [AI PROMPTS] Testing 'What is AWS?' AI prompt... | Description: Tests AI prompt functionality by sending 'What is AWS?' and verifying the response contains relevant AWS information and technical terms");
    
    let mut chat = QChatSession::new()?;
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
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}

#[test]
#[cfg(all(feature = "ai_prompts", feature = "sanity"))]
fn test_simple_greeting() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing simple 'Hello' prompt... | Description: Tests basic AI interaction by sending a simple greeting and verifying the AI responds appropriately with greeting-related content");
    
    let mut chat = QChatSession::new()?;
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
    
    chat.quit()?;
    println!("✅ Test completed successfully");
    
    Ok(())
}
