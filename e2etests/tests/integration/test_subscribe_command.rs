#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;


#[test]
#[cfg(all(feature = "subscribe", feature = "sanity"))]
fn test_subscribe_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /subscribe command... | Description: Tests the <code> /subscribe</code> command to display Q Developer Pro subscription information and IAM Identity Center details");

    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let response = chat.execute_command_with_timeout("/subscribe",Some(500))?;
    
    println!("📝 Subscribe response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify subscription management message
    assert!(response.contains("Q Developer Pro subscription") && response.contains("IAM Identity Center"), "Missing subscription management message");
    println!("✅ Found subscription management message");
    
    println!("✅ All subscribe content verified!");

    drop(chat);
    
    Ok(())
}

#[test]
#[cfg(all(feature = "subscribe", feature = "sanity"))]
fn test_subscribe_manage_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /subscribe --manage command... | Description: Tests the <code> /subscribe --manage</code> command to access subscription management interface for Q Developer Pro");

    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let response = chat.execute_command_with_timeout("/subscribe --manage",Some(500))?;
    
    println!("📝 Subscribe response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify subscription management message
    assert!(response.contains("Q Developer Pro subscription") && response.contains("IAM Identity Center"), "Missing subscription management message");
    println!("✅ Found subscription management message");
    
    println!("✅ All subscribe content verified!");

    drop(chat);
    
    Ok(())
}

#[test]
#[cfg(all(feature = "subscribe", feature = "sanity"))]
fn test_subscribe_help_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /subscribe --help command... | Description: Tests the <code> /subscribe --help</code> command to display comprehensive help information for subscription management");

    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let response = chat.execute_command_with_timeout("/subscribe --help",Some(500))?;
    
    println!("📝 Subscribe help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify description
    assert!(response.contains("Q Developer Pro subscription"), "Missing subscription description");
    println!("✅ Found subscription description");
    
    // Verify Usage section
    assert!(response.contains("Usage:"), "Missing Usage section");
    assert!(response.contains("/subscribe"), "Missing /subscribe command in usage section");
    assert!(response.contains("[OPTIONS]"), "Missing [OPTIONS] in usage section");
    println!("✅ Found Usage section with /subscribe [OPTIONS]");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    println!("✅ Found Options section");
    
    // Verify manage option
    assert!(response.contains("--manage"), "Missing --manage option");
    println!("✅ Found --manage option");
    
    // Verify help flags
    assert!(response.contains("-h") &&  response.contains("--help"), "Missing -h, --help flags");
    println!("✅ Found help flags: -h, --help with Print help description");
    
    println!("✅ All subscribe help content verified!");
    
    drop(chat);

    Ok(())
}

#[test]
#[cfg(all(feature = "subscribe", feature = "sanity"))]
fn test_subscribe_h_command() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing /subscribe -h command... | Description: Tests the <code> /subscribe -h</code> command (short form) to display subscription help information");

    let session = q_chat_helper::get_chat_session();
    let mut chat = session.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

    let response = chat.execute_command_with_timeout("/subscribe -h",Some(500))?;
    
    println!("📝 Subscribe help response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    // Verify description
    assert!(response.contains("Q Developer Pro subscription"), "Missing subscription description");
    println!("✅ Found subscription description");
    
    // Verify Usage section
    assert!(response.contains("Usage:"), "Missing Usage section");
    assert!(response.contains("/subscribe"), "Missing /subscribe command in usage section");
    assert!(response.contains("[OPTIONS]"), "Missing [OPTIONS] in usage section");
    println!("✅ Found Usage section with /subscribe [OPTIONS]");
    
    // Verify Options section
    assert!(response.contains("Options:"), "Missing Options section");
    println!("✅ Found Options section");
    
    // Verify manage option
    assert!(response.contains("--manage"), "Missing --manage option");
    println!("✅ Found --manage option");
    
    // Verify help flags
    assert!(response.contains("-h") &&  response.contains("--help"), "Missing -h, --help flags");
    println!("✅ Found help flags: -h, --help with Print help description");
    
    println!("✅ All subscribe help content verified!");
    
    drop(chat);

    Ok(())
}