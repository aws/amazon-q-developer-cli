// Standalone program to index autodocs for introspect
// Run once to enable semantic search in introspect tool

use std::path::PathBuf;

use semantic_search_client::AsyncSemanticSearchClient;
use semantic_search_client::config::SemanticSearchConfig;
use semantic_search_client::types::AddContextRequest;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Indexing autodocs for introspect semantic search...\n");

    // Use same path as introspect tool
    let home = dirs::home_dir().ok_or("Could not find home directory")?;
    let doc_search_dir = home.join(".kiro").join("doc-search");

    println!("Doc search directory: {}", doc_search_dir.display());

    // Create client with config
    let config = SemanticSearchConfig {
        base_dir: doc_search_dir.clone(),
        chunk_size: 100000, // Very large to avoid chunking docs
        chunk_overlap: 0,
        ..Default::default()
    };

    let client = AsyncSemanticSearchClient::with_config(&doc_search_dir, config).await?;
    println!("✓ Client created\n");

    // Check if already indexed
    let contexts = client.get_contexts().await;
    if contexts.iter().any(|c| c.name == "kiro-autodocs") {
        println!("✓ autodocs already indexed");
        println!("\nExisting contexts:");
        for ctx in contexts {
            println!("  - {}: {} items", ctx.name, ctx.item_count);
        }
        return Ok(());
    }

    // Index autodocs
    let autodocs_path = PathBuf::from("autodocs/docs");
    if !autodocs_path.exists() {
        eprintln!("Error: autodocs/docs not found");
        eprintln!("Run this from the kiro-cli project root");
        std::process::exit(1);
    }

    println!("Indexing: {}", autodocs_path.display());

    let request = AddContextRequest {
        path: autodocs_path,
        name: "kiro-autodocs".to_string(),
        description: "Kiro CLI documentation".to_string(),
        persistent: true,
        include_patterns: Some(vec!["**/*.md".to_string()]),
        exclude_patterns: None,
        embedding_type: None,
        auto_sync: false,
    };

    let (op_id, _) = client.add_context(request).await?;
    println!("✓ Indexing started: {}\n", op_id);

    // Wait for completion
    println!("Waiting for indexing to complete...");
    loop {
        tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

        let status = client.get_status_data().await?;
        println!(
            "  Active operations: {}, Total contexts: {}",
            status.active_count, status.total_contexts
        );

        if status.active_count == 0 {
            break;
        }
    }

    // Verify
    let contexts = client.get_contexts().await;
    println!("\n✓ Indexing complete!");
    println!("\nContexts:");
    for ctx in &contexts {
        println!("  - {}: {} items", ctx.name, ctx.item_count);
    }

    if contexts.iter().any(|c| c.name == "kiro-autodocs") {
        println!("\n✅ Success! Introspect will now use semantic search");
    } else {
        println!("\n❌ Failed to create kiro-autodocs context");
    }

    Ok(())
}
