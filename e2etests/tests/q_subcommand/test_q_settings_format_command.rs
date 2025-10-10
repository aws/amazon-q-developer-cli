use q_cli_e2e_tests::q_chat_helper;

/// Tests the 'q settings --format' subcommand with the following:
/// - Verifies that the command returns a non-empty response
/// - Checks that the response contains the expected JSON-formatted setting value
/// - Validates that the setting name is referenced in the output
/// - Uses json-pretty format to display the chat.defaultAgent setting
#[test]
#[cfg(all(feature = "q_subcommand", feature = "sanity"))]
fn test_q_setting_format_subcommand() -> Result<(), Box<dyn std::error::Error>> {

println!("\n🔍 Testing q settings --format <FORMAT> <SETTINGS>... | Description: Tests the q settings --FORMAT <FORMAT> <SETTINGS> subcommand to validate FORMAT content.");
let response = q_chat_helper::execute_q_subcommand("q", &["settings", "--format", "json-pretty", "chat.defaultAgent"])?;

println!("📝 transform response: {} bytes", response.len());
println!("📝 FULL OUTPUT:");
println!("{}", response);
println!("📝 END OUTPUT");

assert!(!response.is_empty(), "Expected non-empty response");
assert!(response.contains("\"q_cli_default\""), "Expected JSON-formatted setting value");
assert!(response.contains("chat.defaultAgent"), "Expected command to reference the setting name");

Ok(())
}
#[test]
#[cfg(all(feature = "q_subcommand", feature = "sanity"))]
fn test_q_setting_format_subcommand() -> Result<(), Box<dyn std::error::Error>> {

println!("\n🔍 Testing q settings --format <FORMAT> <SETTINGS>... | Description: Tests the q settings --FORMAT <FORMAT> <SETTINGS> subcommand to validate FORMAT content.");
let response = q_chat_helper::execute_q_subcommand("q", &["settings", "--format", "json-pretty", "chat.defaultAgent"])?;

println!("📝 transform response: {} bytes", response.len());
println!("📝 FULL OUTPUT:");
println!("{}", response);
println!("📝 END OUTPUT");

assert!(!response.is_empty(), "Expected non-empty response");
assert!(response.contains("\"q_cli_default\""), "Expected JSON-formatted setting value");
assert!(response.contains("chat.defaultAgent"), "Expected command to reference the setting name");

Ok(())
}