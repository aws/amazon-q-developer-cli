//! Tests for TreeSitterSymbolService
//!
//! Comprehensive test coverage for symbol extraction, search, and workspace analysis.

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::super::tree_sitter_symbol_service::TreeSitterSymbolService;
    use crate::model::types::{
        FindSymbolsRequest,
        GenerateCodebaseOverviewRequest,
        SearchCodebaseMapRequest,
    };

    #[test]
    fn test_get_extensions_for_lang() {
        let exts = crate::tree_sitter::get_extensions("rust").unwrap_or(&[]);
        assert!(exts.iter().any(|ext| ext == "rs"));
    }

    #[tokio::test]
    async fn test_typescript_class_detection() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.ts");

        let content = r#"
export class MyTestClass {
    private name: string;
    
    constructor(name: string) {
        this.name = name;
    }
    
    public getName(): string {
        return this.name;
    }
}
"#;
        fs::write(&file_path, content).unwrap();

        let mut workspace_manager = crate::sdk::workspace_manager::WorkspaceManager::new(temp_dir.path().to_path_buf());
        let service = TreeSitterSymbolService::new();

        let request = FindSymbolsRequest {
            symbol_name: "MyTestClass".to_string(),
            file_path: None,
            symbol_type: None,
            language: None,
            limit: Some(10),
            exact_match: false,
            timeout_secs: None,
        };

        let result = service.find_symbols(&mut workspace_manager, &request).await;

        assert!(result.is_ok());
        let symbols = result.unwrap();
        assert!(!symbols.is_empty(), "Should find MyTestClass");

        let class_symbol = symbols.iter().find(|s| s.name == "MyTestClass");
        assert!(class_symbol.is_some(), "Should find MyTestClass symbol");
    }

    #[tokio::test]
    async fn test_rust_symbol_extraction() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("lib.rs");

        let content = r#"
pub struct User {
    pub name: String,
    pub age: u32,
}

impl User {
    pub fn new(name: String, age: u32) -> Self {
        User { name, age }
    }
}

pub fn public_function() -> i32 {
    42
}

fn private_function() {
    println!("private");
}
"#;
        fs::write(&file_path, content).unwrap();

        let mut workspace_manager = crate::sdk::workspace_manager::WorkspaceManager::new(temp_dir.path().to_path_buf());
        let service = TreeSitterSymbolService::new();

        // Test document symbols
        let symbols = service.get_document_symbols(&mut workspace_manager, &file_path).await;
        assert!(symbols.is_ok());
        let symbols = symbols.unwrap();
        assert!(!symbols.is_empty());

        let symbol_names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(symbol_names.contains(&"User"), "Should find User struct");
        assert!(symbol_names.contains(&"public_function"), "Should find public_function");
    }

    #[tokio::test]
    async fn test_symbol_container_source_line_and_position() {
        let temp_dir = TempDir::new().unwrap();

        // Test Rust impl method
        let rust_file = temp_dir.path().join("service.rs");
        let rust_content =
            "impl UserService {\n    fn get_user(&self, id: u32, name: String) -> User {\n        todo!()\n    }\n}\n";
        fs::write(&rust_file, rust_content).unwrap();

        // Test TypeScript class method
        let ts_file = temp_dir.path().join("api.ts");
        let ts_content = "class ApiService {\n    async fetchData(url: string, opts: RequestInit): Promise<Response> {\n        return fetch(url);\n    }\n}\n";
        fs::write(&ts_file, ts_content).unwrap();

        let mut workspace_manager = crate::sdk::workspace_manager::WorkspaceManager::new(temp_dir.path().to_path_buf());
        let service = TreeSitterSymbolService::new();

        // Test Rust
        let rust_symbols = service
            .get_document_symbols(&mut workspace_manager, &rust_file)
            .await
            .unwrap();
        let get_user = rust_symbols
            .iter()
            .find(|s| s.name == "get_user")
            .expect("should find get_user");
        assert_eq!(
            get_user.container_name,
            Some("UserService".to_string()),
            "method should have container"
        );
        assert_eq!(get_user.start_row, 2, "method starts on line 2");
        assert_eq!(get_user.start_column, 5, "method starts at column 5");
        let source = get_user.source_line.as_ref().expect("should have source_line");
        assert!(
            source.contains("fn get_user"),
            "source_line should contain function name"
        );
        assert!(source.contains("id: u32"), "source_line should contain arguments");
        assert!(!source.contains('\n'), "source_line should be single line");

        // Test TypeScript
        let ts_symbols = service
            .get_document_symbols(&mut workspace_manager, &ts_file)
            .await
            .unwrap();
        let fetch_data = ts_symbols
            .iter()
            .find(|s| s.name == "fetchData")
            .expect("should find fetchData");
        assert_eq!(
            fetch_data.container_name,
            Some("ApiService".to_string()),
            "method should have container"
        );
        assert_eq!(fetch_data.start_row, 2, "method starts on line 2");
        assert_eq!(fetch_data.start_column, 5, "method starts at column 5");
        let source = fetch_data.source_line.as_ref().expect("should have source_line");
        assert!(
            source.contains("async fetchData"),
            "source_line should contain method signature"
        );
        assert!(source.contains("url: string"), "source_line should contain arguments");

        // Test top-level has no container
        let impl_block = rust_symbols
            .iter()
            .find(|s| s.name == "UserService")
            .expect("should find impl");
        assert_eq!(
            impl_block.container_name, None,
            "top-level impl should have no container"
        );
    }

    #[tokio::test]
    async fn test_find_symbols_empty_query() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("lib.rs");

        let content = r#"
pub struct User {
    pub name: String,
    pub age: u32,
}

impl User {
    pub fn new(name: String, age: u32) -> Self {
        User { name, age }
    }
}

pub fn public_function() -> i32 {
    42
}

fn private_function() {
    println!("private");
}
"#;
        fs::write(&file_path, content).unwrap();

        let mut workspace_manager = crate::sdk::workspace_manager::WorkspaceManager::new(temp_dir.path().to_path_buf());
        let service = TreeSitterSymbolService::new();

        let request = FindSymbolsRequest {
            symbol_name: "User".to_string(),
            file_path: None,
            symbol_type: None,
            language: None,
            limit: Some(100),
            exact_match: false,
            timeout_secs: None,
        };

        let result = service.find_symbols(&mut workspace_manager, &request).await;
        assert!(result.is_ok());
        let symbols = result.unwrap();

        let symbol_names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(symbol_names.contains(&"User"), "Should find User struct");
    }

    #[tokio::test]
    async fn test_python_function_detection() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.py");

        let content = r#"
class Calculator:
    def __init__(self):
        self.result = 0
    
    def add(self, x, y):
        return x + y
    
    def multiply(self, x, y):
        return x * y

def standalone_function():
    return "hello"
"#;
        fs::write(&file_path, content).unwrap();

        let mut workspace_manager = crate::sdk::workspace_manager::WorkspaceManager::new(temp_dir.path().to_path_buf());
        let service = TreeSitterSymbolService::new();

        let request = FindSymbolsRequest {
            symbol_name: "Calculator".to_string(),
            file_path: None,
            symbol_type: None,
            language: Some("python".to_string()),
            limit: Some(10),
            exact_match: false,
            timeout_secs: None,
        };

        let result = service.find_symbols(&mut workspace_manager, &request).await;
        assert!(result.is_ok());
        let symbols = result.unwrap();

        let class_symbol = symbols.iter().find(|s| s.name == "Calculator");
        assert!(class_symbol.is_some(), "Should find Calculator class");
    }

    #[tokio::test]
    async fn test_generate_codebase_overview() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("main.rs");

        let content = r#"
fn main() {
    println!("Hello, world!");
}
"#;
        fs::write(&file_path, content).unwrap();

        let mut workspace_manager = crate::sdk::workspace_manager::WorkspaceManager::new(temp_dir.path().to_path_buf());
        let service = TreeSitterSymbolService::new();

        let request = GenerateCodebaseOverviewRequest {
            path: None,
            timeout_secs: None,
            token_budget: None,
        };

        let result = service
            .generate_codebase_overview(&mut workspace_manager, &request)
            .await;
        assert!(result.is_ok());
        let overview = result.unwrap();

        assert!(!overview.workspace_path.is_empty());
        assert!(overview.summary.total_files > 0);
    }

    #[tokio::test]
    async fn test_search_codebase_map() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("main.rs");

        let content = r#"
fn main() {
    println!("Hello, world!");
}
"#;
        fs::write(&file_path, content).unwrap();

        let mut workspace_manager = crate::sdk::workspace_manager::WorkspaceManager::new(temp_dir.path().to_path_buf());
        let service = TreeSitterSymbolService::new();

        let request = SearchCodebaseMapRequest {
            timeout_secs: None,
            token_budget: None,
            path: None,
            file_path: Some("main.rs".to_string()),
        };

        let result = service.search_codebase_map(&mut workspace_manager, &request).await;
        assert!(result.is_ok());
        let map = result.unwrap();

        assert!(map.files_processed > 0);
        assert!(!map.condensed_repomap.is_empty());
    }

    #[tokio::test]
    async fn test_search_codebase_map_token_count() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("example.rs");

        let content = r#"
pub struct User {
    name: String,
}

impl User {
    pub fn new(name: String) -> Self {
        User { name }
    }
    
    pub fn get_name(&self) -> &str {
        &self.name
    }
}

pub fn helper() {}
"#;
        fs::write(&file_path, content).unwrap();

        let mut workspace_manager = crate::sdk::workspace_manager::WorkspaceManager::new(temp_dir.path().to_path_buf());
        let service = TreeSitterSymbolService::new();

        let request = SearchCodebaseMapRequest {
            timeout_secs: None,
            token_budget: None,
            path: None,
            file_path: None,
        };

        let result = service.search_codebase_map(&mut workspace_manager, &request).await;
        assert!(result.is_ok());
        let map = result.unwrap();

        // Verify token count is calculated (not 0)
        assert!(
            map.token_count > 0,
            "Token count should be greater than 0, got: {}",
            map.token_count
        );
        assert!(map.files_processed > 0);
        assert!(!map.condensed_repomap.is_empty());

        // Token count should be roughly repomap length / 4
        let expected_tokens = map.condensed_repomap.len() / 4;
        assert!(map.token_count > 0 && map.token_count <= expected_tokens + 10);
    }

    #[tokio::test]
    async fn test_search_codebase_map_java_files() {
        let temp_dir = TempDir::new().unwrap();

        // Create Java source file
        let java_file = temp_dir.path().join("User.java");
        let java_content = r#"
package com.example;

public class User {
    private String name;
    
    public User(String name) {
        this.name = name;
    }
    
    public String getName() {
        return name;
    }
    
    public void setName(String name) {
        this.name = name;
    }
}
"#;
        fs::write(&java_file, java_content).unwrap();

        // Create Gradle build file
        let gradle_file = temp_dir.path().join("build.gradle");
        fs::write(&gradle_file, "// Gradle build file").unwrap();

        let mut workspace_manager = crate::sdk::workspace_manager::WorkspaceManager::new(temp_dir.path().to_path_buf());
        let service = TreeSitterSymbolService::new();

        let request = SearchCodebaseMapRequest {
            timeout_secs: None,
            token_budget: None,
            path: None,
            file_path: None,
        };

        let result = service.search_codebase_map(&mut workspace_manager, &request).await;
        assert!(result.is_ok());
        let map = result.unwrap();

        // Should find Java file, not just Gradle
        assert!(
            map.files_processed >= 1,
            "Should process at least the Java file, got: {}",
            map.files_processed
        );
        assert!(
            map.condensed_repomap.contains("User.java"),
            "Should contain Java file in repomap"
        );
        assert!(
            map.condensed_repomap.contains("getName") || map.condensed_repomap.contains("User"),
            "Should contain Java symbols in repomap"
        );
        assert!(map.token_count > 0, "Token count should be greater than 0");
    }

    #[tokio::test]
    async fn test_search_codebase_map_max_depth() {
        let temp_dir = TempDir::new().unwrap();

        // Create deeply nested Java file (6 levels deep - typical Java structure)
        let deep_dir = temp_dir
            .path()
            .join("src")
            .join("main")
            .join("java")
            .join("com")
            .join("example")
            .join("project");
        fs::create_dir_all(&deep_dir).unwrap();

        let deep_java_file = deep_dir.join("DeepClass.java");
        fs::write(&deep_java_file, "public class DeepClass { public void method() {} }").unwrap();

        // Create shallow Java file (2 levels deep)
        let shallow_dir = temp_dir.path().join("src");
        let shallow_java_file = shallow_dir.join("ShallowClass.java");
        fs::write(
            &shallow_java_file,
            "public class ShallowClass { public void method() {} }",
        )
        .unwrap();

        let mut workspace_manager = crate::sdk::workspace_manager::WorkspaceManager::new(temp_dir.path().to_path_buf());
        let service = TreeSitterSymbolService::new();

        let request = SearchCodebaseMapRequest {
            timeout_secs: None,
            token_budget: None,
            path: None,
            file_path: None,
        };

        let result = service.search_codebase_map(&mut workspace_manager, &request).await;
        assert!(result.is_ok());
        let map = result.unwrap();

        // Should find both shallow and deep files with max_depth=10
        assert!(
            map.condensed_repomap.contains("ShallowClass"),
            "Should find ShallowClass at depth 2"
        );
        assert!(
            map.condensed_repomap.contains("DeepClass"),
            "Should find DeepClass at depth 6 with max_depth=10"
        );
        assert!(map.files_processed >= 2, "Should process both Java files");
    }

    #[tokio::test]
    async fn test_fuzzy_search_partial_match() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("service.rs");

        let content = r#"
pub struct UserService {
    pub name: String,
}

pub struct AuthenticationService {
    pub token: String,
}
"#;
        fs::write(&file_path, content).unwrap();

        let mut workspace_manager = crate::sdk::workspace_manager::WorkspaceManager::new(temp_dir.path().to_path_buf());
        let service = TreeSitterSymbolService::new();

        // Test partial match "Serv" should find both services
        let request = FindSymbolsRequest {
            symbol_name: "Serv".to_string(),
            file_path: None,
            symbol_type: None,
            language: None,
            limit: Some(10),
            exact_match: false,
            timeout_secs: None,
        };

        let result = service.find_symbols(&mut workspace_manager, &request).await;
        assert!(result.is_ok());
        let symbols = result.unwrap();

        assert!(symbols.len() >= 2, "Should find both services with fuzzy match");
        let names: Vec<&str> = symbols.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"UserService"));
        assert!(names.contains(&"AuthenticationService"));
    }

    #[tokio::test]
    async fn test_multi_language_workspace() {
        let temp_dir = TempDir::new().unwrap();

        // Create Rust file
        let rust_file = temp_dir.path().join("main.rs");
        fs::write(&rust_file, "pub fn rust_function() {}").unwrap();

        // Create TypeScript file
        let ts_file = temp_dir.path().join("app.ts");
        fs::write(&ts_file, "export function tsFunction() {}").unwrap();

        // Create Python file
        let py_file = temp_dir.path().join("script.py");
        fs::write(&py_file, "def python_function(): pass").unwrap();

        let mut workspace_manager = crate::sdk::workspace_manager::WorkspaceManager::new(temp_dir.path().to_path_buf());
        let service = TreeSitterSymbolService::new();

        // Search for "function" should find symbols from all languages
        let request = FindSymbolsRequest {
            symbol_name: "function".to_string(),
            file_path: None,
            symbol_type: None,
            language: None,
            limit: Some(10),
            exact_match: false,
            timeout_secs: None,
        };

        let result = service.find_symbols(&mut workspace_manager, &request).await;
        assert!(result.is_ok());
        let symbols = result.unwrap();

        assert!(symbols.len() >= 3, "Should find functions from multiple languages");
    }

    #[tokio::test]
    async fn test_error_handling_unsupported_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("data.txt");

        // Create a non-source file
        fs::write(&file_path, "This is just text data").unwrap();

        let mut workspace_manager = crate::sdk::workspace_manager::WorkspaceManager::new(temp_dir.path().to_path_buf());
        let service = TreeSitterSymbolService::new();

        // Should handle unsupported file gracefully
        let result = service.get_document_symbols(&mut workspace_manager, &file_path).await;
        assert!(result.is_err(), "Should return error for unsupported file type");
    }
}
