// Async tests for semantic search client
mod tests {
    use std::env;
    use std::sync::Arc;
    use std::sync::atomic::{
        AtomicUsize,
        Ordering,
    };
    use std::time::Duration;

    use semantic_search_client::SemanticSearchClient;
    use semantic_search_client::types::ProgressStatus;
    use tempfile::TempDir;
    use tokio::{
        task,
        time,
    };

    #[tokio::test]
    async fn test_background_indexing_example() {
        if env::var("MEMORY_BANK_USE_REAL_EMBEDDERS").is_err() {
            println!("Skipping test: MEMORY_BANK_USE_REAL_EMBEDDERS not set");
            return;
        }
        // Create a temp directory that will live for the duration of the test
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_path_buf();

        // Create a test file with unique content
        let unique_id = uuid::Uuid::new_v4().to_string();
        let test_file = temp_path.join("test.txt");
        let content = format!("This is a unique test document {unique_id} for semantic search");
        std::fs::write(&test_file, &content).unwrap();

        // Example of background indexing using tokio::task::spawn_blocking
        let path_clone = test_file.clone();
        let name = format!("Test Context {unique_id}");
        let description = "Test Description";
        let persistent = true;

        // Spawn a background task for indexing
        let handle = task::spawn(async move {
            task::spawn_blocking(move || {
                // Create a new client inside the blocking task
                let mut client = SemanticSearchClient::new_with_default_dir().unwrap();
                client.add_context_from_path(
                    &path_clone,
                    &name,
                    description,
                    persistent,
                    Option::<fn(ProgressStatus)>::None,
                )
            })
            .await
            .unwrap()
            .unwrap()
        });

        // Wait for the background task to complete
        let context_id = handle.await.unwrap();
        println!("Created context with ID: {context_id}");

        // Wait a moment for indexing to complete
        time::sleep(Duration::from_millis(500)).await;

        // Create another client to search the newly created context
        let search_client = SemanticSearchClient::new_with_default_dir().unwrap();

        // Search for the unique content
        let results = search_client.search_all(&unique_id, None).unwrap();

        // Verify we can find our content
        assert!(!results.is_empty(), "Expected to find our test document");

        // This demonstrates how to perform background indexing using tokio tasks
        // while still being able to use the synchronous client
    }

    #[tokio::test]
    async fn test_background_indexing_with_progress() {
        if env::var("MEMORY_BANK_USE_REAL_EMBEDDERS").is_err() {
            println!("Skipping test: MEMORY_BANK_USE_REAL_EMBEDDERS not set");
            return;
        }
        // Create a temp directory for our test files
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_path_buf();

        // Create multiple test files with unique content
        let unique_id = uuid::Uuid::new_v4().to_string();
        let unique_id_clone = unique_id.clone(); // Clone for later use
        let num_files = 10;

        for i in 0..num_files {
            let file_path = temp_path.join(format!("test_file_{i}.txt"));
            let content = format!(
                "This is test file {i} with unique ID {unique_id} for semantic search.\n\n\
                 It contains multiple paragraphs to test chunking.\n\n\
                 This is paragraph 3 with some additional content.\n\n\
                 And finally paragraph 4 with more text for embedding."
            );
            std::fs::write(&file_path, &content).unwrap();
        }

        // Create a progress counter to track indexing progress
        let progress_counter = Arc::new(AtomicUsize::new(0));
        let progress_counter_clone = Arc::clone(&progress_counter);

        // Create a progress callback
        let progress_callback = move |status: ProgressStatus| match status {
            ProgressStatus::CountingFiles => {
                println!("Counting files...");
            },
            ProgressStatus::StartingIndexing(count) => {
                println!("Starting indexing of {count} files...");
            },
            ProgressStatus::Indexing(current, total) => {
                println!("Indexing file {current}/{total}");
                progress_counter_clone.store(current, Ordering::SeqCst);
            },
            ProgressStatus::DownloadingModel(current, total) => {
                println!("Downloading model {current}/{total} bytes");
            },
            ProgressStatus::CreatingSemanticContext => {
                println!("Creating semantic context...");
            },
            ProgressStatus::GeneratingEmbeddings(current, total) => {
                println!("Generating embeddings {current}/{total}");
            },
            ProgressStatus::BuildingIndex => {
                println!("Building index...");
            },
            ProgressStatus::Finalizing => {
                println!("Finalizing...");
            },
            ProgressStatus::Complete => {
                println!("Indexing complete!");
            },
        };

        // Spawn a background task for indexing the directory
        let handle = task::spawn(async move {
            task::spawn_blocking(move || {
                // Create a new client inside the blocking task
                let mut client = SemanticSearchClient::new_with_default_dir().unwrap();
                client.add_context_from_path(
                    &temp_path,
                    &format!("Large Test Context {unique_id}"),
                    "Test with multiple files and progress tracking",
                    true,
                    Some(progress_callback),
                )
            })
            .await
            .unwrap()
            .unwrap()
        });

        // While the indexing is happening, we can do other work
        // For this test, we'll just periodically check the progress
        let mut last_progress = 0;
        for _ in 0..10 {
            time::sleep(Duration::from_millis(100)).await;
            let current_progress = progress_counter.load(Ordering::SeqCst);
            if current_progress > last_progress {
                println!("Progress update: {current_progress} files processed");
                last_progress = current_progress;
            }
        }

        // Wait for the background task to complete
        let context_id = handle.await.unwrap();
        println!("Created context with ID: {context_id}");

        // Wait a moment for indexing to complete
        time::sleep(Duration::from_millis(500)).await;

        // Create another client to search the newly created context
        let search_client = SemanticSearchClient::new_with_default_dir().unwrap();

        // Search for the unique content
        let results = search_client.search_all(&unique_id_clone, None).unwrap();

        // Verify we can find our content
        assert!(!results.is_empty(), "Expected to find our test documents");

        // Verify that we can search for specific content in specific files
        for i in 0..num_files {
            let file_specific_query = format!("test file {i}");
            let file_results = search_client.search_all(&file_specific_query, None).unwrap();
            assert!(!file_results.is_empty(), "Expected to find test file {i}");
        }
    }
}
