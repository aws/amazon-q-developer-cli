#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;

#[test]
#[cfg(all(feature = "experiment", feature = "sanity"))]
fn test_knowledge_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /experiment command... | Description: Tests the <code>  /experiment </code> command to toggle Knowledge experimental features");
    
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    println!("✅ Q Chat session started");
    
    let response = chat.execute_command_with_timeout("/experiment",Some(500))?;
    
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
    let revert_response = chat.execute_command_with_timeout("/experiment",Some(500))?;
    
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
    
    drop(chat);
    
    Ok(())
}

#[test]
#[cfg(all(feature = "experiment", feature = "sanity"))]
fn test_thinking_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /experiment command... | Description: Tests the <code>  /experiment </code> command to toggle thinking experimental features");
    
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    println!("✅ Q Chat session started");
    
    let response = chat.execute_command_with_timeout("/experiment",Some(500))?;
    
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
    let revert_response = chat.execute_command_with_timeout("/experiment",Some(500))?;
    
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
    
    drop(chat);
    
    Ok(())
}

#[test]
#[cfg(all(feature = "experiment", feature = "sanity"))]
fn test_experiment_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /experiment --help command... | Description: Tests the <code> /experiment --help</code> command to display help information");

    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    println!("✅ Q Chat session started");
    
    let response = chat.execute_command_with_timeout("/experiment --help",Some(500))?;
    
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
    
    drop(chat);
    
    Ok(())
}

#[test]
#[cfg(all(feature = "experiment", feature = "sanity"))]
fn test_tangent_mode_experiment() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing Tangent Mode experiment... | Description: Tests the <code> /experiment </code> command to toggle Tangent Mode experimental feature");
    
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    println!("✅ Q Chat session started");
    
    let response = chat.execute_command_with_timeout("/experiment",Some(500))?;
    
    println!("📝 Experiment response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify experiment menu content
    assert!(response.contains("Select"), "Missing selection prompt");
    assert!(response.contains("Tangent Mode"), "Missing Tangent Mode experiment");
    println!("✅ Found experiment menu with Tangent Mode option");
    
    // Find Tangent Mode and check if it's already selected
    let lines: Vec<&str> = response.lines().collect();
    let mut Tangent_menu_position = 0;
    let mut Tangent_state = false;
    let mut found = false;
    let mut Tangent_already_selected = false;
    
    // Check if Tangent Mode is already selected (has ❯)
    for line in lines.iter() {
        if line.contains("Tangent Mode") && line.trim_start().starts_with("❯") {
            Tangent_already_selected = true;
            Tangent_state = line.contains("[ON]");
            found = true;
            break;
        }
    }
    
    // If not selected, find its position
    if !Tangent_already_selected {
        let mut menu_position = 0;
        for line in lines.iter() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("❯") || (trimmed.contains("[ON]") || trimmed.contains("[OFF]")) {
                if line.contains("Tangent Mode") {
                    Tangent_menu_position = menu_position;
                    Tangent_state = line.contains("[ON]");
                    found = true;
                    break;
                }
                menu_position += 1;
            }
        }
    }
    
    assert!(found, "Tangent Mode option not found in menu");
    println!("📝 Tangent Mode already selected: {}, position: {}, state: {}", Tangent_already_selected, Tangent_menu_position, if Tangent_state { "ON" } else { "OFF" });
    
    // Navigate to Tangent Mode option using arrow keys (only if not already selected)
    if !Tangent_already_selected {
        for _ in 0..Tangent_menu_position {
            chat.send_key_input("\x1b[B")?; // Down arrow
        }
    }
    
    // Select the Tangent Mode option
    let navigate_response = chat.send_key_input("\r")?; // Enter
    
    println!("📝 Navigate response: {} bytes", navigate_response.len());
    println!("📝 NAVIGATE RESPONSE:");
    println!("{}", navigate_response);
    println!("📝 END NAVIGATE RESPONSE");
    
    // Verify toggle response based on previous state
    if Tangent_state {
        assert!(navigate_response.contains("Tangent Mode experiment disabled"), "Expected Tangent Mode to be disabled");
        println!("✅ Tangent Mode experiment disabled successfully");
    } else {
        assert!(navigate_response.contains("Tangent Mode experiment enabled"), "Expected Tangent Mode to be enabled");
        println!("✅ Tangent Mode experiment enabled successfully");
    }
    
    // Test reverting back to original state (run command again)
    println!("📝 Testing revert to original state...");
    let revert_response = chat.execute_command_with_timeout("/experiment",Some(500))?;
    
    // Navigate to Tangent Mode option again (only if not already selected)
    if !Tangent_already_selected {
        for _ in 0..Tangent_menu_position {
            chat.send_key_input("\x1b[B")?; // Down arrow
        }
    }
    let revert_navigate_response = chat.send_key_input("\r")?; // Enter
    
    println!("📝 Revert response: {} bytes", revert_navigate_response.len());
    println!("📝 REVERT RESPONSE:");
    println!("{}", revert_navigate_response);
    println!("📝 END REVERT RESPONSE");
    
    // Verify it reverted to original state
    if Tangent_state {
        assert!(revert_navigate_response.contains("Tangent Mode experiment enabled"), "Expected Tangent Mode to be enabled (reverted)");
        println!("✅ Tangent Mode experiment reverted to enabled successfully");
    } else {
        assert!(revert_navigate_response.contains("Tangent Mode experiment disabled"), "Expected Tangent Mode to be disabled (reverted)");
        println!("✅ Tangent Mode experiment reverted to disabled successfully");
    }

    println!("✅ Tangent Mode experiment test completed successfully");
    
    drop(chat);
    
    Ok(())
}

#[test]
#[cfg(all(feature = "experiment", feature = "sanity"))]
fn test_todo_lists_experiment() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing Todo Lists experiment... | Description: Tests the <code> /experiment </code> command to toggle Todo Lists experimental feature");
    
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    println!("✅ Q Chat session started");
    
    let response = chat.execute_command_with_timeout("/experiment",Some(500))?;
    
    println!("📝 Experiment response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify experiment menu content
    assert!(response.contains("Select"), "Missing selection prompt");
    assert!(response.contains("Todo Lists"), "Missing Todo Lists experiment");
    println!("✅ Found experiment menu with Todo Lists option");
    
    // Find Todo Lists and check if it's already selected
    let lines: Vec<&str> = response.lines().collect();
    let mut TodoLists_menu_position = 0;
    let mut TodoLists_state = false;
    let mut found = false;
    let mut TodoLists_already_selected = false;
    
    // Check if Todo Lists is already selected (has ❯)
    for line in lines.iter() {
        if line.contains("Todo Lists") && line.trim_start().starts_with("❯") {
            TodoLists_already_selected = true;
            TodoLists_state = line.contains("[ON]");
            found = true;
            break;
        }
    }
    
    // If not selected, find its position
    if !TodoLists_already_selected {
        let mut menu_position = 0;
        for line in lines.iter() {
            let trimmed = line.trim_start();
            if trimmed.starts_with("❯") || (trimmed.contains("[ON]") || trimmed.contains("[OFF]")) {
                if line.contains("Todo Lists") {
                    TodoLists_menu_position = menu_position;
                    TodoLists_state = line.contains("[ON]");
                    found = true;
                    break;
                }
                menu_position += 1;
            }
        }
    }
    
    assert!(found, "Todo Lists option not found in menu");
    println!("📝 Todo Lists already selected: {}, position: {}, state: {}", TodoLists_already_selected, TodoLists_menu_position, if TodoLists_state { "ON" } else { "OFF" });
    
    // Navigate to Todo Lists option using arrow keys (only if not already selected)
    if !TodoLists_already_selected {
        for _ in 0..TodoLists_menu_position {
            chat.send_key_input("\x1b[B")?; // Down arrow
        }
    }
    
    // Select the Todo Lists option
    let navigate_response = chat.send_key_input("\r")?; // Enter
    
    println!("📝 Navigate response: {} bytes", navigate_response.len());
    println!("📝 NAVIGATE RESPONSE:");
    println!("{}", navigate_response);
    println!("📝 END NAVIGATE RESPONSE");
    
    // Verify toggle response based on previous state
    if TodoLists_state {
        assert!(navigate_response.contains("Todo Lists experiment disabled"), "Expected Todo Lists to be disabled");
        println!("✅ Todo Lists experiment disabled successfully");
    } else {
        assert!(navigate_response.contains("Todo Lists experiment enabled"), "Expected Todo Lists to be enabled");
        println!("✅ Todo Lists experiment enabled successfully");
    }
    
    // Test reverting back to original state (run command again)
    println!("📝 Testing revert to original state...");
    let revert_response = chat.execute_command_with_timeout("/experiment",Some(500))?;
    
    // Navigate to Todo Lists option again (only if not already selected)
    if !TodoLists_already_selected {
        for _ in 0..TodoLists_menu_position {
            chat.send_key_input("\x1b[B")?; // Down arrow
        }
    }
    let revert_navigate_response = chat.send_key_input("\r")?; // Enter
    
    println!("📝 Revert response: {} bytes", revert_navigate_response.len());
    println!("📝 REVERT RESPONSE:");
    println!("{}", revert_navigate_response);
    println!("📝 END REVERT RESPONSE");
    
    // Verify it reverted to original state
    if TodoLists_state {
        assert!(revert_navigate_response.contains("Todo Lists experiment enabled"), "Expected Todo Lists to be enabled (reverted)");
        println!("✅ Todo Lists experiment reverted to enabled successfully");
    } else {
        assert!(revert_navigate_response.contains("Todo Lists experiment disabled"), "Expected Todo Lists to be disabled (reverted)");
        println!("✅ Todo Lists experiment reverted to disabled successfully");
    }

    println!("✅ Todo Lists experiment test completed successfully");
    
    drop(chat);
    
    Ok(())
}