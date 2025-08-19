use q_cli_e2e_tests::q_chat_helper::QChatSession;

#[test]
#[cfg(feature = "model")]
fn test_all_model_commands() -> Result<(), Box<dyn std::error::Error>> {
    let mut chat = QChatSession::new()?;
    println!(":white_check_mark: Q Chat session started");
    
    test_model_dynamic_command(&mut chat)?;
    test_model_help_command(&mut chat)?;
    
    chat.quit()?;
    println!(":white_check_mark: All tests completed successfully");
    Ok(())
}
fn test_model_dynamic_command(chat: &mut QChatSession) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /model command with dynamic selection...");
    
    let mut chat = QChatSession::new()?;
    println!("✅ Q Chat session started");
    
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
            println!("🔍 Row: '{}' -> Cleaned: '{}'", trimmed_line, cleaned_line);
            
            if !trimmed_line.is_empty() {
                // Check if line contains a model (starts with ❯, spaces, or contains model names)
                if cleaned_line.starts_with("❯") || cleaned_line.starts_with(" ") || cleaned_line.contains("-") {
                    let model_name = cleaned_line
                        .replace("❯", "")
                        .replace("(active)", "")
                        .trim()
                        .to_string();
                    
                    println!("🔍 Extracted model: '{}'", model_name);
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

    Ok(())
}

fn test_model_help_command(chat: &mut QChatSession) -> Result<(), Box<dyn std::error::Error>> {
    println!("🔍 Testing /model --help command...");
    
    
    let response = chat.execute_command("/model --help")?;
    
    println!("📝 Model help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    /* Verify description
    assert!(response.contains("Select") && response.contains("model"), "Missing model selection description");
    println!("✅ Found model selection description");*/
    
    // Verify Usage section
    assert!(response.contains("Usage:"), "Missing Usage section");
    assert!(response.contains("/model"), "Missing /model command in usage section");
    println!("✅ Found Usage section with /model command");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    println!("✅ Found Options section");
    
    // Verify help flags
    assert!(response.contains("-h") &&  response.contains("--help") && response.contains("Print help"), "Missing -h, --help flags");
    assert!(response.contains("Print help"), "Missing Print help description");
    println!("✅ Found help flags: -h, --help with Print help description");
    
    println!("✅ All model help content verified!");
    
    Ok(())
}
