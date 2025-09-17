#[allow(unused_imports)]
use q_cli_e2e_tests::q_chat_helper;

#[test]
#[cfg(all(feature = "q_subcommand", feature = "sanity"))]
fn test_q_doctor_subcommand() -> Result<(), Box<dyn std::error::Error>> {
    println!("\n🔍 Testing q doctor subcommand... | Description: Tests the <code> q doctor </code> subcommand that debugs installation issues");

    println!("\n🔍 Executing 'q doctor' subcommand...");
    let response = q_chat_helper::execute_q_subcommand("q", &["doctor"])?;
    
    println!("📝 Doctor response: {} bytes", response.len());
    println!("📝 FULL OUTPUT:");
    println!("{}", response);
    println!("📝 END OUTPUT");
    
    assert!(response.contains("q issue"), "Missing troubleshooting message");
    println!("✅ Found troubleshooting message");
    
    if response.contains("Everything looks good!") {
        println!("✅ Doctor check passed - everything looks good!");
    }
    
    println!("✅ Doctor subcommand output verified!");
    
    Ok(())
}
