#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;

#[test]
#[cfg(all(feature = "todos", feature = "sanity"))]
fn test_todos_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /todos command... | Description: Tests the <code> /todos</code> command to view, manage, and resume to-do lists");
    
    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    println!("✅ Q Chat session started");
    
    let response = chat.execute_command("/todos")?;
    
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
    
    let response = chat.execute_command("/todos help")?;
    
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
    
    let response = chat.execute_command("Add task in todos list Review emails")?;
    
    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify help content
    assert!(response.contains("Using tool"), "Missing tool usage confirmation");
    assert!(response.contains("todo_list"), "Missing todo_list tool usage");
    assert!(response.contains("Review emails"), "Missing Review emails message");
    println!("✅ Confirmed todo_list tool usage");

    let response = chat.execute_command("/todos view")?;

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

    let response = chat.execute_command("/todos delete")?;

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

    let response = chat.execute_command("Add task in todos list Review emails")?;
    
    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify help content
    assert!(response.contains("Using tool"), "Missing tool usage confirmation");
    assert!(response.contains("todo_list"), "Missing todo_list tool usage");
    assert!(response.contains("Review emails"), "Missing Review emails message");
    println!("✅ Confirmed todo_list tool usage");

    let response = chat.execute_command("/todos resume")?;

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

    let response = chat.execute_command("/todos delete")?;

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

    let response = chat.execute_command("Add task in todos list Review emails")?;
    
    println!("📝 Help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify help content
    assert!(response.contains("Using tool"), "Missing tool usage confirmation");
    assert!(response.contains("todo_list"), "Missing todo_list tool usage");
    assert!(response.contains("Review emails"), "Missing Review emails message");
    println!("✅ Confirmed todo_list tool usage");

    let response = chat.execute_command("/todos view")?;

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

    let response = chat.execute_command("/todos delete")?;

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
