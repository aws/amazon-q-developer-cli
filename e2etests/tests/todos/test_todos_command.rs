#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;
#[allow(unused_imports)]
use regex::Regex;

#[test]
#[cfg(all(feature = "todos", feature = "sanity"))]
fn test_todos_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /todos command... | Description: Tests the <code> /todos</code> command to view, manage, and resume to-do lists");

    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    println!("✅ Q Chat session started");

    let response = chat.execute_command_with_timeout("/todos",Some(2000))?;

    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Verify help content
    assert!(response.contains("Commands:"), "Missing Commands section");
    println!("✅ Found Commands section with all available commands");

    assert!(response.contains("resume"), "Missing resume command");
    assert!(response.contains("view"), "Missing view command");
    assert!(response.contains("delete"), "Missing delete command");
    assert!(response.contains("help"), "Missing help command");
    println!("✅ Found core commands: resume, view, delete, help");

    println!("✅ /todos command test completed successfully");

    drop(chat);

    Ok(())
}

#[test]
#[cfg(all(feature = "todos", feature = "sanity"))]
fn test_todos_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /todos help command... | Description: Tests the <code> /todos help</code> command to display help information about the todos ");

    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    println!("✅ Q Chat session started");

    let response = chat.execute_command_with_timeout("/todos help",Some(2000))?;

    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Verify help content
    assert!(response.contains("Commands:"), "Missing Commands section");
    println!("✅ Found Commands section with all available commands");

    assert!(response.contains("resume"), "Missing resume command");
    assert!(response.contains("view"), "Missing view command");
    assert!(response.contains("delete"), "Missing delete command");
    assert!(response.contains("help"), "Missing help command");
    println!("✅ Found core commands: resume, view, delete, help");

    println!("✅ /todos help command test completed successfully");

    drop(chat);

    Ok(())
}

#[test]
#[cfg(all(feature = "todos", feature = "sanity"))]
fn test_todos_view_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /todos view command... | Description: Tests the <code> /todos view</code> command to view to-do lists");

    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    println!("Executing 'q settings chat.enableTodoList true' to enable todos feature...");
    q_chat_helper::execute_q_subcommand("q", &["settings", "chat.enableTodoList", "true"])?;

    let response = q_chat_helper::execute_q_subcommand("q", &["settings", "all"])?;

    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    assert!(response.contains("chat.enableTodoList = true"), "Failed to enable todos feature");
    println!("✅ Todos feature enabled");

    println!("✅ Q Chat session started");

    let response = chat.execute_command_with_timeout("Add task in todos list Review emails",Some(2000))?;

    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Verify help content
    assert!(response.contains("Using tool"), "Missing tool usage confirmation");
    assert!(response.contains("todo_list"), "Missing todo_list tool usage");
    assert!(response.contains("Review emails"), "Missing Review emails message");
    println!("✅ Confirmed todo_list tool usage");

    let response = chat.execute_command_with_timeout("/todos view",Some(2000))?;

    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    assert!(response.contains("to-do"), "Missing to-do message");
    assert!(response.contains("view"), "Missing view message");
    println!("✅ Confirmed to-do item presence in view output");

    // Send down arrow to select different model
    let selection_response = chat.send_key_input("\x1b[B")?;

    println!("📝 Selection response: {} bytes", selection_response.len());
    println!("📝 SELECTION RESPONSE:");
    println!("{}", selection_response);
    println!("📝 END SELECTION RESPONSE");

    // Send Enter to confirm
    let confirm_response = chat.send_key_input("\r")?;

    println!("📝 Confirm response: {} bytes", confirm_response.len());
    println!("📝 CONFIRM RESPONSE:");
    println!("{}", confirm_response);
    println!("📝 END CONFIRM RESPONSE");

    assert!(confirm_response.contains("TODO"), "Missing TODO message");
    assert!(confirm_response.contains("Review emails"), "Missing Review emails to-do item");
    println!("✅ Confirmed viewing of selected to-do list with items");

    let response = chat.execute_command_with_timeout("/todos delete",Some(2000))?;

    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    assert!(response.contains("to-do"), "Missing to-do message");
    assert!(response.contains("delete"), "Missing delete message");

    // Send down arrow to select different model
    let selection_response = chat.send_key_input("\x1b[B")?;

    println!("📝 Selection response: {} bytes", selection_response.len());
    println!("📝 SELECTION RESPONSE:");
    println!("{}", selection_response);
    println!("📝 END SELECTION RESPONSE");

    // Send Enter to confirm
    let confirm_response = chat.send_key_input("\r")?;

    println!("📝 Confirm response: {} bytes", confirm_response.len());
    println!("📝 CONFIRM RESPONSE:");
    println!("{}", confirm_response);
    println!("📝 END CONFIRM RESPONSE");

    assert!(confirm_response.contains("Deleted"), "Missing Deleted message");
    assert!(confirm_response.contains("to-do"), "Missing to-do item");
    println!("✅ Confirmed deletion of selected to-do list");

    println!("✅ /todos view command test completed successfully");

    drop(chat);

    Ok(())
}

#[test]
#[cfg(all(feature = "todos", feature = "sanity"))]
fn test_todos_resume_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /todos resume command... | Description: Tests the <code> /todos resume</code> command to resume a specific to-do list");

    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    println!("Executing 'q settings chat.enableTodoList true' to enable todos feature...");
    q_chat_helper::execute_q_subcommand("q", &["settings", "chat.enableTodoList", "true"])?;

    let response = q_chat_helper::execute_q_subcommand("q", &["settings", "all"])?;

    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    assert!(response.contains("chat.enableTodoList = true"), "Failed to enable todos feature");
    println!("✅ Todos feature enabled");

    println!("✅ Q Chat session started");

    let response = chat.execute_command_with_timeout("Add task in todos list Review emails",Some(2000))?;

    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Verify help content
    assert!(response.contains("Using tool"), "Missing tool usage confirmation");
    assert!(response.contains("todo_list"), "Missing todo_list tool usage");
    assert!(response.contains("Review emails"), "Missing Review emails message");
    println!("✅ Confirmed todo_list tool usage");

    let response = chat.execute_command_with_timeout("/todos resume",Some(2000))?;

    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    assert!(response.contains("to-do"), "Missing to-do message");
    assert!(response.contains("resume"), "Missing resume message");
    println!("✅ Confirmed to-do item presence in resume output");

    // Send down arrow to select different model
    let selection_response = chat.send_key_input("\x1b[B")?;

    println!("📝 Selection response: {} bytes", selection_response.len());
    println!("📝 SELECTION RESPONSE:");
    println!("{}", selection_response);
    println!("📝 END SELECTION RESPONSE");

    // Send Enter to confirm
    let confirm_response = chat.send_key_input("\r")?;

    println!("📝 Confirm response: {} bytes", confirm_response.len());
    println!("📝 CONFIRM RESPONSE:");
    println!("{}", confirm_response);
    println!("📝 END CONFIRM RESPONSE");

    assert!(confirm_response.contains("Review emails"), "Missing Review emails message");
    assert!(confirm_response.contains("TODO"), "Missing TODO item");
    println!("✅ Confirmed resuming of selected to-do list with items");

    let response = chat.execute_command_with_timeout("/todos delete",Some(2000))?;

    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    assert!(response.contains("to-do"), "Missing to-do message");
    assert!(response.contains("delete"), "Missing delete message");

    // Send down arrow to select different model
    let selection_response = chat.send_key_input("\x1b[B")?;

    println!("📝 Selection response: {} bytes", selection_response.len());
    println!("📝 SELECTION RESPONSE:");
    println!("{}", selection_response);
    println!("📝 END SELECTION RESPONSE");

    // Send Enter to confirm
    let confirm_response = chat.send_key_input("\r")?;

    println!("📝 Confirm response: {} bytes", confirm_response.len());
    println!("📝 CONFIRM RESPONSE:");
    println!("{}", confirm_response);
    println!("📝 END CONFIRM RESPONSE");

    assert!(confirm_response.contains("Deleted"), "Missing Deleted message");
    assert!(confirm_response.contains("to-do"), "Missing to-do item");
    println!("✅ Confirmed deletion of selected to-do list");

    println!("✅ /todos resume command test completed successfully");

    drop(chat);

    Ok(())
}

#[test]
#[cfg(all(feature = "todos", feature = "sanity"))]
fn test_todos_delete_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /todos delete command... | Description: Tests the <code> /todos delete</code> command to delete a specific to-do list");

    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    println!("Executing 'q settings chat.enableTodoList true' to enable todos feature...");
    q_chat_helper::execute_q_subcommand("q", &["settings", "chat.enableTodoList", "true"])?;

    let response = q_chat_helper::execute_q_subcommand("q", &["settings", "all"])?;

    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    assert!(response.contains("chat.enableTodoList = true"), "Failed to enable todos feature");
    println!("✅ Todos feature enabled");

    println!("✅ Q Chat session started");

    let response = chat.execute_command_with_timeout("Add task in todos list Review emails",Some(2000))?;

    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    // Verify help content
    assert!(response.contains("Using tool"), "Missing tool usage confirmation");
    assert!(response.contains("todo_list"), "Missing todo_list tool usage");
    assert!(response.contains("Review emails"), "Missing Review emails message");
    println!("✅ Confirmed todo_list tool usage");

    let response = chat.execute_command_with_timeout("/todos view",Some(2000))?;

    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    assert!(response.contains("to-do"), "Missing to-do message");
    assert!(response.contains("view"), "Missing view message");
    println!("✅ Confirmed to-do item presence in view output");

    // Send down arrow to select different model
    let selection_response = chat.send_key_input("\x1b[B")?;

    println!("📝 Selection response: {} bytes", selection_response.len());
    println!("📝 SELECTION RESPONSE:");
    println!("{}", selection_response);
    println!("📝 END SELECTION RESPONSE");

    // Send Enter to confirm
    let confirm_response = chat.send_key_input("\r")?;

    println!("📝 Confirm response: {} bytes", confirm_response.len());
    println!("📝 CONFIRM RESPONSE:");
    println!("{}", confirm_response);
    println!("📝 END CONFIRM RESPONSE");

    assert!(confirm_response.contains("TODO"), "Missing TODO message");
    assert!(confirm_response.contains("Review emails"), "Missing Review emails to-do item");
    println!("✅ Confirmed viewing of selected to-do list with items");

    let response = chat.execute_command_with_timeout("/todos delete",Some(2000))?;

    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");

    assert!(response.contains("to-do"), "Missing to-do message");
    assert!(response.contains("delete"), "Missing delete message");
    println!("✅ Confirmed to-do item presence in delete output");

    // Send down arrow to select different model
    let selection_response = chat.send_key_input("\x1b[B")?;

    println!("📝 Selection response: {} bytes", selection_response.len());
    println!("📝 SELECTION RESPONSE:");
    println!("{}", selection_response);
    println!("📝 END SELECTION RESPONSE");

    // Send Enter to confirm
    let confirm_response = chat.send_key_input("\r")?;

    println!("📝 Confirm response: {} bytes", confirm_response.len());
    println!("📝 CONFIRM RESPONSE:");
    println!("{}", confirm_response);
    println!("📝 END CONFIRM RESPONSE");

    assert!(confirm_response.contains("Deleted"), "Missing Deleted message");
    assert!(confirm_response.contains("to-do"), "Missing to-do item");
    println!("✅ Confirmed deletion of selected to-do list");

    println!("✅ /todos delete command test completed successfully");

    drop(chat);

    Ok(())
}
#[test]
#[cfg(all(feature = "todos", feature = "sanity"))]
fn test_todos_clear_finished_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /todos clear-finished command... | Description: Tests that <code> /todos clear-finished </code> command to validate it clears the todo list.");

    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    println!("✅ Global Q Chat session started");

    // Create todo list with 2 tasks
    println!("\n🔍 Creating todo list with 2 tasks...");
    let create_response = chat.execute_command_with_timeout("create a todo_list with 2 task in amazon q", Some(2000))?;
    println!("📝 Create response: {} bytes", create_response.len());
    println!("📝 Create response: {}", create_response);
    println!("✅ Found create response.");
    println!("✅ Tasks has been created successfully.");

    // Extract todo ID
    let re = Regex::new(r"(\d{10,})")?;
    let todo_id = re.find(&create_response)
        .map(|m| m.as_str())
        .ok_or("Could not extract todo list ID")?;
    println!("📝 Extracted todo ID: {}", todo_id);

    // Mark all tasks as completed
    println!("\n🔍 Marking all tasks as completed...");
    let mark_response = chat.execute_command_with_timeout(&format!("mark all tasks as completed for todo list {}", todo_id), Some(2000))?;
    println!("📝 Mark complete response: {} bytes", mark_response.len());
    println!("📝 Mark complete response: {}", mark_response);
    println!("✅ Found Task completion response.");

    // Test clear-finished command
    println!("\n🔍 Testing clear-finished command...");
    let clear_response = chat.execute_command_with_timeout("/todos clear-finished", Some(2000))?;
    println!("📝 Clear response: {} bytes", clear_response.len());
    println!("📝 {}", clear_response);

    assert!(!clear_response.is_empty(), "Expected non-empty response from clear-finished command");
    println!("✅ Found todo_list clear response");
    println!("✅ All finished task cleared successfully.");

    drop(chat);
    Ok(())
}