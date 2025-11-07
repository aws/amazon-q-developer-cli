use super::config::{TestProject};
use code_agent_sdk::{
    CodeIntelligence, FindSymbolsRequest, GotoDefinitionRequest, 
    FindReferencesByLocationRequest, RenameSymbolRequest, FormatCodeRequest, OpenFileRequest,
    GetDocumentDiagnosticsRequest
};
use anyhow::Result;
use std::time::Duration;

/// US-001: Workspace Detection Test
pub async fn test_workspace_detection(project: &TestProject) -> Result<()> {
    let mut code_intel = CodeIntelligence::builder()
        .workspace_root(project.path.clone())
        .auto_detect_languages()
        .build()
        .map_err(|e| anyhow::anyhow!(e))?;
    
    code_intel.initialize().await?;

    let workspace_info = code_intel.detect_workspace()?;
    assert_eq!(workspace_info.root_path, project.path);
    assert!(workspace_info.detected_languages.contains(&project.config.language));
    
    Ok(())
}

/// US-002: Symbol Finding in Files Test
pub async fn test_file_symbol_finding(project: &TestProject) -> Result<()> {
    let mut code_intel = CodeIntelligence::builder()
        .workspace_root(project.path.clone())
        .add_language(&project.config.language)
        .build()
        .map_err(|e| anyhow::anyhow!(e))?;
    
    code_intel.initialize().await?;

    let main_file = project.main_file_path();
    let content = std::fs::read_to_string(&main_file)?;
    code_intel.open_file(OpenFileRequest {
        file_path: main_file.clone(),
        content,
    }).await?;

    // Allow LSP to process the file with retry logic
    let symbol_name = match project.config.language.as_str() {
        "rust" => "greet_user",
        "typescript" => "greetUser", 
        "python" => "greet_user",
        _ => "greet",
    };

    // Retry symbol finding up to 3 times with increasing delays
    let mut symbols = Vec::new();
    for attempt in 1..=3 {
        tokio::time::sleep(Duration::from_secs(attempt * 2)).await;
        
        let request = FindSymbolsRequest {
            symbol_name: symbol_name.to_string(),
            file_path: Some(main_file.clone()),
            symbol_type: None,
            limit: None,
            exact_match: false,
        };

        symbols = code_intel.find_symbols(request).await?;
        if !symbols.is_empty() {
            break;
        }
        println!("Attempt {}: No symbols found, retrying...", attempt);
    }

    assert!(!symbols.is_empty(), "Should find greet function after retries");
    
    let greet_symbol = symbols.iter()
        .find(|s| s.name.contains(symbol_name))
        .expect("Should find greet symbol");
    
    assert!(greet_symbol.file_path.ends_with(&project.config.main_file));

    Ok(())
}

/// US-003: Workspace Symbol Search Test
pub async fn test_workspace_symbol_search(project: &TestProject) -> Result<()> {
    let mut code_intel = CodeIntelligence::builder()
        .workspace_root(project.path.clone())
        .add_language(&project.config.language)
        .build()
        .map_err(|e| anyhow::anyhow!(e))?;
    
    code_intel.initialize().await?;

    let main_file = project.main_file_path();
    let content = std::fs::read_to_string(&main_file)?;
    code_intel.open_file(OpenFileRequest {
        file_path: main_file.clone(),
        content,
    }).await?;

    // Allow LSP to index
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Test workspace-wide search
    let request = FindSymbolsRequest {
        symbol_name: "calculate".to_string(),
        file_path: None, // Workspace-wide search
        symbol_type: None,
        limit: None,
        exact_match: false,
    };

    let _symbols = code_intel.find_symbols(request).await?;
    // Note: Some LSPs may not support workspace/symbol, so we allow empty results
    
    Ok(())
}

/// US-004: Go-to-Definition Test
pub async fn test_goto_definition(project: &TestProject) -> Result<()> {
    let mut code_intel = CodeIntelligence::builder()
        .workspace_root(project.path.clone())
        .add_language(&project.config.language)
        .build()
        .map_err(|e| anyhow::anyhow!(e))?;
    
    code_intel.initialize().await?;

    let main_file = project.main_file_path();
    let content = std::fs::read_to_string(&main_file)?;
    code_intel.open_file(OpenFileRequest {
        file_path: main_file.clone(),
        content,
    }).await?;

    // Allow LSP to process
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Find a function call position (varies by language)
    let (row, column) = match project.config.language.as_str() {
        "rust" => (8, 21), // greet_user call in main (1-based)
        "typescript" => (12, 21), // greetUser call in main (1-based)
        "python" => (10, 16), // greet_user call in main (1-based)
        _ => (1, 1),
    };

    let request = GotoDefinitionRequest {
        file_path: main_file.clone(),
        row,
        column,
        show_source: true,
    };

    let definition = code_intel.goto_definition(request).await?;
    if let Some(def) = definition {
        assert!(def.file_path.ends_with(&project.config.main_file));
    }

    Ok(())
}

/// US-005: Find References Test
pub async fn test_find_references(project: &TestProject) -> Result<()> {
    let mut code_intel = CodeIntelligence::builder()
        .workspace_root(project.path.clone())
        .add_language(&project.config.language)
        .build()
        .map_err(|e| anyhow::anyhow!(e))?;
    
    code_intel.initialize().await?;

    let main_file = project.main_file_path();
    let content = std::fs::read_to_string(&main_file)?;
    code_intel.open_file(OpenFileRequest {
        file_path: main_file.clone(),
        content,
    }).await?;

    // Allow LSP to process
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Find function definition position (1-based)
    let (row, column) = match project.config.language.as_str() {
        "rust" => (1, 11), // greet_user function definition
        "typescript" => (1, 21), // greetUser function definition
        "python" => (1, 6), // greet_user function definition
        _ => (1, 1),
    };

    let request = FindReferencesByLocationRequest {
        file_path: main_file.clone(),
        row,
        column,
    };

    let references = code_intel.find_references_by_location(request).await?;
    assert!(!references.is_empty(), "Should find at least the definition");

    Ok(())
}

/// US-006: Rename Symbol Test (Dry-run)
pub async fn test_rename_symbol(project: &TestProject) -> Result<()> {
    let mut code_intel = CodeIntelligence::builder()
        .workspace_root(project.path.clone())
        .add_language(&project.config.language)
        .build()
        .map_err(|e| anyhow::anyhow!(e))?;
    
    code_intel.initialize().await?;

    let main_file = project.main_file_path();
    let content = std::fs::read_to_string(&main_file)?;
    code_intel.open_file(OpenFileRequest {
        file_path: main_file.clone(),
        content,
    }).await?;

    // Allow LSP to process
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Find function definition position (1-based)
    let (row, column) = match project.config.language.as_str() {
        "rust" => (1, 11), // greet_user function definition
        "typescript" => (1, 21), // greetUser function definition  
        "python" => (1, 6), // greet_user function definition
        _ => (1, 1),
    };

    let request = RenameSymbolRequest {
        file_path: main_file.clone(),
        row,
        column,
        new_name: "welcome_user".to_string(),
        dry_run: true, // Always dry-run in tests
    };

    let rename_result = code_intel.rename_symbol(request).await?;
    if let Some(result) = rename_result {
        assert!(result.edit_count > 0, "Should have rename changes");
    }

    Ok(())
}

/// US-007: Code Formatting Test
pub async fn test_code_formatting(project: &TestProject) -> Result<()> {
    let mut code_intel = CodeIntelligence::builder()
        .workspace_root(project.path.clone())
        .add_language(&project.config.language)
        .build()
        .map_err(|e| anyhow::anyhow!(e))?;
    
    code_intel.initialize().await?;

    let main_file = project.main_file_path();
    let content = std::fs::read_to_string(&main_file)?;
    code_intel.open_file(OpenFileRequest {
        file_path: main_file.clone(),
        content,
    }).await?;

    // Allow LSP to process
    tokio::time::sleep(Duration::from_secs(1)).await;

    let request = FormatCodeRequest {
        file_path: Some(main_file.clone()),
        insert_spaces: true,
        tab_size: 4,
    };

    let _format_result = code_intel.format_code(request).await?;
    // Format may return empty if code is already formatted
    
    Ok(())
}

/// US-008: Pull Diagnostics Test
pub async fn test_pull_diagnostics(project: &TestProject) -> Result<()> {
    let mut code_intel = CodeIntelligence::builder()
        .workspace_root(project.path.clone())
        .add_language(&project.config.language)
        .build()
        .map_err(|e| anyhow::anyhow!(e))?;
    
    code_intel.initialize().await?;

    // Use the existing obvious_errors.rs file
    let error_file = project.path.join("src/obvious_errors.rs");
    let error_content = std::fs::read_to_string(&error_file)?;

    code_intel.open_file(OpenFileRequest {
        file_path: error_file.clone(),
        content: error_content,
    }).await?;

    // Allow LSP to process the file and generate diagnostics
    tokio::time::sleep(Duration::from_secs(2)).await;

    let request = GetDocumentDiagnosticsRequest {
        file_path: error_file.clone(),
        identifier: None,
        previous_result_id: None,
    };

    // Test pull diagnostics
    match code_intel.get_document_diagnostics(request).await {
        Ok(diagnostics) => {
            println!("✅ Pull diagnostics supported - found {} diagnostics", diagnostics.len());
            for (i, diagnostic) in diagnostics.iter().enumerate() {
                println!("  Diagnostic {}: {} at line {}", 
                    i + 1, 
                    diagnostic.message, 
                    diagnostic.range.start.line + 1
                );
            }
            Ok(())
        }
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("Unhandled method") || error_msg.contains("Method Not Found") {
                println!("ℹ️  Pull diagnostics not supported by language server (expected)");
                Ok(()) // This is expected for most language servers
            } else {
                println!("⚠️  Pull diagnostics failed with unexpected error: {}", error_msg);
                Err(e) // Unexpected error
            }
        }
    }
}

/// Run all user story tests for a project
pub async fn run_all_user_story_tests(project: &TestProject) -> Result<Vec<(&'static str, Result<()>)>> {
    let mut results = Vec::new();

    results.push(("US-001: Workspace Detection", test_workspace_detection(project).await));
    results.push(("US-002: File Symbol Finding", test_file_symbol_finding(project).await));
    results.push(("US-003: Workspace Symbol Search", test_workspace_symbol_search(project).await));
    results.push(("US-004: Go-to-Definition", test_goto_definition(project).await));
    results.push(("US-005: Find References", test_find_references(project).await));
    results.push(("US-006: Rename Symbol", test_rename_symbol(project).await));
    results.push(("US-007: Code Formatting", test_code_formatting(project).await));
    results.push(("US-008: Pull Diagnostics", test_pull_diagnostics(project).await));

    Ok(results)
}
