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
    "test_todos_command",
    "test_todos_help_command",
    "test_todos_view_command",
    "test_todos_resume_command",
    "test_todos_delete_command"
];
#[allow(dead_code)]
const TOTAL_TESTS: usize = TEST_NAMES.len();

#[test]
#[cfg(all(feature = "todos", feature = "sanity"))]
fn test_todos_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /todos command... | Description: Tests the <code> /todos</code> command to view, manage, and resume to-do lists");
    
    let session = get_chat_session();
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
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(all(feature = "todos", feature = "sanity"))]
fn test_todos_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /todos help command... | Description: Tests the <code> /todos help</code> command to display help information about the todos ");

    let session = get_chat_session();
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
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(all(feature = "todos", feature = "sanity"))]
fn test_todos_view_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /todos view command... | Description: Tests the <code> /todos view</code> command to view to-do lists");
       
    let session = get_chat_session();
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
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(all(feature = "todos", feature = "sanity"))]
fn test_todos_resume_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /todos resume command... | Description: Tests the <code> /todos resume</code> command to resume a specific to-do list");
       
    let session = get_chat_session();
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
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}

#[test]
#[cfg(all(feature = "todos", feature = "sanity"))]
fn test_todos_delete_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /todos delete command... | Description: Tests the <code> /todos delete</code> command to delete a specific to-do list");
       
    let session = get_chat_session();
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
    
    // Release the lock before cleanup
    drop(chat);
    
    // Cleanup only if this is the last test
    cleanup_if_last_test(&TEST_COUNT, TOTAL_TESTS)?;
    
    Ok(())
}
