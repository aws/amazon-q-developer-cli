#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;

#[test]
#[cfg(all(feature = "q_subcommand", feature = "sanity"))]
fn test_q_quit_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!(
        "\n🔍 Testing q settings q quit subcommand | Description: Tests the <code>q quit </code> subcommand to validate whether it quit the amazon q app."
    );
    // Launch Amazon Q app.
    println!("Launching Q...");
    let launch_response = q_chat_helper::execute_q_subcommand("q", &["launch"])?;
    println!("📝 Debug response: {} bytes", launch_response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", launch_response);
    println!("📝 END OUTPUT");

    assert!(launch_response.contains("Opening Amazon Q dashboard"),"Missing amazon Q opening message");

    // Quit Amazon q app.
    println!("Quitting Q...");
    let quit_response = q_chat_helper::execute_q_subcommand("q", &["quit"])?;
    println!("📝 Debug response: {} bytes", quit_response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", quit_response);
    println!("📝 END OUTPUT");

    assert!(quit_response.contains("Quitting Amazon Q app"), "Missing amazon Q quit message");
    Ok(())

}