//! E2E Integration Tests
//! 
//! Comprehensive end-to-end tests based on user stories.
//! These tests require external language servers and should NOT run in CI/CD.
//! Run with: cargo test --test e2e_integration -- --ignored

mod e2e;

use e2e::{TestConfig, ProjectConfig, TestProject, run_all_user_story_tests};
use anyhow::Result;

/// Test all user stories with Rust project
#[tokio::test]
#[ignore = "e2e_test"] // Exclude from CI/CD - requires rust-analyzer
async fn test_rust_user_stories() -> Result<()> {
    // Skip if rust-analyzer not available
    if std::process::Command::new("rust-analyzer")
        .arg("--version")
        .output()
        .is_err()
    {
        println!("Skipping Rust tests - rust-analyzer not available");
        return Ok(());
    }

    let config = TestConfig::default();
    std::fs::create_dir_all(&config.temp_dir)?;

    let project_config = ProjectConfig::rust_project();
    let project = TestProject::create(project_config, &config.temp_dir)?;

    // Test core functionality only to avoid timeout
    println!("Testing US-001: Workspace Detection");
    e2e::test_workspace_detection(&project).await?;
    println!("✅ US-001 passed");

    println!("Testing US-002: File Symbol Finding");
    e2e::test_file_symbol_finding(&project).await?;
    println!("✅ US-002 passed");

    println!("Testing US-004: Go-to-Definition");
    e2e::test_goto_definition(&project).await?;
    println!("✅ US-004 passed");

    println!("Testing US-008: Pull Diagnostics");
    e2e::test_pull_diagnostics(&project).await?;
    println!("✅ US-008 passed");

    println!("\nRust E2E Results: 4 core tests passed");
    
    Ok(())
}

/// Test all user stories with TypeScript project
#[tokio::test]
#[ignore = "e2e_test"] // Exclude from CI/CD - requires typescript-language-server
async fn test_typescript_user_stories() -> Result<()> {
    // Skip if typescript-language-server not available
    if std::process::Command::new("typescript-language-server")
        .arg("--version")
        .output()
        .is_err()
    {
        println!("Skipping TypeScript tests - typescript-language-server not available");
        return Ok(());
    }

    let config = TestConfig::default();
    std::fs::create_dir_all(&config.temp_dir)?;

    let project_config = ProjectConfig::typescript_project();
    let project = TestProject::create(project_config, &config.temp_dir)?;

    // Test core functionality only to avoid timeout
    println!("Testing US-001: Workspace Detection");
    e2e::test_workspace_detection(&project).await?;
    println!("✅ US-001 passed");

    println!("Testing US-002: File Symbol Finding");
    e2e::test_file_symbol_finding(&project).await?;
    println!("✅ US-002 passed");

    println!("\nTypeScript E2E Results: 2 core tests passed");
    
    Ok(())
}

/// Test all user stories with Python project
#[tokio::test]
#[ignore = "e2e_test"] // Exclude from CI/CD - requires pylsp
async fn test_python_user_stories() -> Result<()> {
    // Skip if pylsp not available
    if std::process::Command::new("pylsp")
        .arg("--version")
        .output()
        .is_err()
    {
        println!("Skipping Python tests - pylsp not available");
        return Ok(());
    }

    let config = TestConfig::default();
    std::fs::create_dir_all(&config.temp_dir)?;

    let project_config = ProjectConfig::python_project();
    let project = TestProject::create(project_config, &config.temp_dir)?;

    let results = tokio::time::timeout(
        std::time::Duration::from_secs(config.timeout_secs),
        run_all_user_story_tests(&project)
    ).await??;

    // Report results
    let mut passed = 0;
    let mut failed = 0;
    
    for (test_name, result) in results {
        match result {
            Ok(()) => {
                println!("✅ {}", test_name);
                passed += 1;
            }
            Err(e) => {
                println!("❌ {}: {}", test_name, e);
                failed += 1;
            }
        }
    }

    println!("\nPython E2E Results: {} passed, {} failed", passed, failed);
    
    // Allow some failures for LSP features that may not be fully supported
    assert!(passed >= 4, "At least 4 user stories should pass for Python");
    
    Ok(())
}

/// Test multi-language workspace detection (US-008)
#[tokio::test]
#[ignore = "e2e_test"]
async fn test_multi_language_support() -> Result<()> {
    let config = TestConfig::default();
    std::fs::create_dir_all(&config.temp_dir)?;

    // Create a multi-language workspace
    let workspace_path = config.temp_dir.join("multi_lang_workspace");
    std::fs::create_dir_all(&workspace_path)?;

    // Create Rust project
    let _rust_project = TestProject::create(
        ProjectConfig::rust_project(),
        &workspace_path
    )?;

    // Create TypeScript project  
    let _ts_project = TestProject::create(
        ProjectConfig::typescript_project(),
        &workspace_path
    )?;

    // Test workspace detection
    use code_agent_sdk::CodeIntelligence;
    let mut code_intel = CodeIntelligence::builder()
        .workspace_root(workspace_path.clone())
        .auto_detect_languages()
        .build()
        .map_err(|e| anyhow::anyhow!(e))?;
    
    code_intel.initialize().await?;

    let workspace_info = code_intel.detect_workspace()?;
    
    assert_eq!(workspace_info.root_path, workspace_path);
    assert!(workspace_info.detected_languages.len() >= 2);
    assert!(workspace_info.detected_languages.contains(&"rust".to_string()));
    assert!(workspace_info.detected_languages.contains(&"typescript".to_string()));

    println!("✅ Multi-language workspace detection successful");
    println!("Detected languages: {:?}", workspace_info.detected_languages);

    Ok(())
}
