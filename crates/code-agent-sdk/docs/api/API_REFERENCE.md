# Code Agent SDK API Reference

## Overview

The Code Agent SDK provides semantic code understanding through Language Server Protocol (LSP) integration. This document covers all APIs, their inputs/outputs, and usage patterns.

## Core Architecture

```
CodeIntelligence → LSP Client → Language Server (rust-analyzer, typescript-language-server, etc.)
```

## Lifecycle Management

### 1. Initialization

```rust
use code_agent_sdk::CodeIntelligence;

// Create instance with builder pattern
let mut code_intel = CodeIntelligence::builder()
    .workspace_root(std::env::current_dir()?)
    .add_language("typescript")
    .add_language("rust")
    .build()
    .map_err(|e| anyhow::anyhow!(e))?;

// Or with auto-detection
let mut code_intel = CodeIntelligence::builder()
    .workspace_root(std::env::current_dir()?)
    .auto_detect_languages()
    .build()
    .map_err(|e| anyhow::anyhow!(e))?;

// Or simple constructor
let mut code_intel = CodeIntelligence::new(std::env::current_dir()?);
```

### 2. Language Server Initialization

Initialize language servers before performing operations:

```rust
code_intel.initialize().await?;
```

### 3. File Management

```rust
use code_agent_sdk::OpenFileRequest;

// Open file for analysis
let content = std::fs::read_to_string("src/main.rs")?;
code_intel.open_file(OpenFileRequest {
    file_path: Path::new("src/main.rs").to_path_buf(),
    content,
}).await?;
```

## API Reference

### Symbol Discovery APIs

#### `find_symbols`

**Purpose**: Fuzzy search for symbols across workspace or specific files.

**Input**: `FindSymbolsRequest`
```rust
pub struct FindSymbolsRequest {
    pub symbol_name: String,        // Search query (empty = all symbols)
    pub file_path: Option<PathBuf>, // Optional: limit to specific file
    pub symbol_type: Option<SymbolKind>, // Optional: filter by symbol type
    pub limit: Option<usize>,       // Optional: max results (default 20, max 50)
    pub exact_match: bool,          // true = exact match, false = fuzzy
}
```

**Output**: `Vec<WorkspaceSymbol>`
```rust
pub struct WorkspaceSymbol {
    pub name: String,
    pub kind: SymbolKind,          // Function, Class, Variable, etc.
    pub tags: Option<Vec<SymbolTag>>,
    pub deprecated: Option<bool>,
    pub location: Location,        // File URI + position range
    pub container_name: Option<String>,
}
```

**Example**:
```rust
let request = FindSymbolsRequest {
    symbol_name: "process".to_string(),
    file_path: None,
    symbol_type: Some(SymbolKind::FUNCTION),
    limit: Some(10),
    exact_match: false,
};
let symbols = code_intel.find_symbols(request).await?;
```

#### `get_symbols`

**Purpose**: Direct symbol retrieval for existence checking or code extraction.

**Input**: `GetSymbolsRequest`
```rust
pub struct GetSymbolsRequest {
    pub symbols: Vec<String>,       // List of symbol names to find
    pub include_source: bool,       // Include source code in response
    pub file_path: Option<PathBuf>, // Optional: limit to specific file
    pub start_row: Option<u32>,     // Optional: search from position
    pub start_column: Option<u32>,
}
```

**Output**: `Vec<WorkspaceSymbol>`

**Example**:
```rust
let request = GetSymbolsRequest {
    symbols: vec!["main".to_string(), "process_data".to_string()],
    include_source: true,
    file_path: Some(PathBuf::from("src/main.rs")),
    start_row: None,
    start_column: None,
};
let symbols = code_intel.get_symbols(request).await?;
```

### Navigation APIs

#### `goto_definition`

**Purpose**: Navigate to symbol definition.

**Input**: File path + position
```rust
pub async fn goto_definition(
    &mut self,
    file_path: &Path,
    line: u32,        // 0-based line number
    character: u32,   // 0-based character position
) -> Result<Option<GotoDefinitionResponse>>
```

**Output**: `Option<GotoDefinitionResponse>`
```rust
pub enum GotoDefinitionResponse {
    Scalar(Location),
    Array(Vec<Location>),
    Link(Vec<LocationLink>),
}

pub struct Location {
    pub uri: Url,
    pub range: Range,
}

pub struct Range {
    pub start: Position,
    pub end: Position,
}
```

**Example**:
```rust
let definition = code_intel.goto_definition(
    Path::new("src/main.rs"),
    10,  // line 10
    15   // character 15
).await?;

if let Some(GotoDefinitionResponse::Scalar(location)) = definition {
    println!("Definition at: {}:{}", location.uri, location.range.start.line);
}
```

### Reference Finding APIs

#### `find_references_by_location`

**Purpose**: Find all references to a symbol at a specific position.

**Input**: `FindReferencesByLocationRequest`
```rust
pub struct FindReferencesByLocationRequest {
    pub file_path: PathBuf,
    pub line: u32,      // 0-based line number
    pub column: u32,    // 0-based column number
}
```

**Output**: `Vec<ReferenceInfo>`
```rust
pub struct ReferenceInfo {
    pub location: Location,
    pub context: Option<String>,
}
```

**Example**:
```rust
let request = FindReferencesByLocationRequest {
    file_path: PathBuf::from("src/main.rs"),
    line: 5,
    column: 10,
};
let references = code_intel.find_references_by_location(request).await?;
```

#### `find_references_by_name`

**Purpose**: Find references by searching for symbol name first.

**Input**: `FindReferencesByNameRequest`
```rust
pub struct FindReferencesByNameRequest {
    pub symbol_name: String,
}
```

**Output**: `Vec<ReferenceInfo>`

**Example**:
```rust
let request = FindReferencesByNameRequest {
    symbol_name: "process_data".to_string(),
};
let references = code_intel.find_references_by_name(request).await?;
```

### Code Modification APIs

#### `rename_symbol`

**Purpose**: Rename symbols with workspace-wide updates.

**Input**: `RenameSymbolRequest`
```rust
pub struct RenameSymbolRequest {
    pub file_path: PathBuf,
    pub start_row: u32,     // 0-based line number
    pub start_column: u32,  // 0-based column number
    pub new_name: String,
    pub dry_run: bool,      // true = preview only, false = apply changes
}
```

**Output**: `Option<WorkspaceEdit>`
```rust
pub struct WorkspaceEdit {
    pub changes: Option<HashMap<Url, Vec<TextEdit>>>,
    pub document_changes: Option<Vec<DocumentChanges>>,
}

pub struct TextEdit {
    pub range: Range,
    pub new_text: String,
}
```

**Example**:
```rust
let request = RenameSymbolRequest {
    file_path: PathBuf::from("src/main.rs"),
    start_row: 10,
    start_column: 5,
    new_name: "new_function_name".to_string(),
    dry_run: true,  // Preview changes
};

if let Some(workspace_edit) = code_intel.rename_symbol(request).await? {
    // Preview changes
    for (uri, edits) in workspace_edit.changes.unwrap_or_default() {
        println!("File: {}", uri);
        for edit in edits {
            println!("  Replace '{}' at line {}", edit.new_text, edit.range.start.line);
        }
    }
}
```

#### `format_code`

**Purpose**: Format code in files or workspace.

**Input**: `FormatCodeRequest`
```rust
pub struct FormatCodeRequest {
    pub file_path: Option<PathBuf>, // None = format workspace
    pub tab_size: u32,              // Default: 4
    pub insert_spaces: bool,        // true = spaces, false = tabs
}
```

**Output**: `Vec<TextEdit>`

**Example**:
```rust
let request = FormatCodeRequest {
    file_path: Some(PathBuf::from("src/main.rs")),
    tab_size: 2,
    insert_spaces: true,
};
let edits = code_intel.format_code(request).await?;

// Apply edits to file
for edit in edits {
    println!("Format change at line {}: '{}'", edit.range.start.line, edit.new_text);
}
```

### Diagnostic APIs

#### `get_diagnostics`

**Purpose**: Get diagnostic information (errors, warnings) for a file.

**Input**: File path
```rust
pub async fn get_diagnostics(&self, file_path: &Path) -> Result<Vec<Diagnostic>>
```

**Output**: `Vec<Diagnostic>`
```rust
pub struct Diagnostic {
    pub range: Range,
    pub severity: Option<DiagnosticSeverity>,
    pub code: Option<NumberOrString>,
    pub message: String,
    pub source: Option<String>,
}
```

**Example**:
```rust
let diagnostics = code_intel.get_diagnostics(Path::new("src/main.rs")).await?;
for diagnostic in diagnostics {
    println!("{}:{} - {}", 
        diagnostic.range.start.line, 
        diagnostic.range.start.character,
        diagnostic.message
    );
}
```

## Complete Usage Example

```rust
use code_agent_sdk::*;
use std::path::Path;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Create instance
    let mut code_intel = CodeIntelligence::with_rust();
    
    let file_path = Path::new("src/main.rs");
    
    // 2. Open file for analysis
    let content = std::fs::read_to_string(file_path)?;
    code_intel.open_file(file_path, content).await?;
    
    // 3. Find all functions in the file
    let find_request = FindSymbolsRequest {
        symbol_name: "".to_string(),
        file_path: Some(file_path.to_path_buf()),
        symbol_type: Some(SymbolKind::FUNCTION),
        limit: None,
        exact_match: false,
    };
    let functions = code_intel.find_symbols(find_request).await?;
    println!("Found {} functions", functions.len());
    
    // 4. Go to definition of first function call
    if let Some(definition) = code_intel.goto_definition(GotoDefinitionRequest { file_path: file_path.to_path_buf(), row: 10, column: 15, show_source: true }).await? {
        println!("Definition found");
    }
    
    // 5. Find all references to a symbol
    let ref_request = FindReferencesByLocationRequest {
        file_path: file_path.to_path_buf(),
        line: 5,
        column: 10,
    };
    let references = code_intel.find_references_by_location(ref_request).await?;
    println!("Found {} references", references.len());
    
    // 6. Format the file
    let format_request = FormatCodeRequest {
        file_path: Some(file_path.to_path_buf()),
        tab_size: 4,
        insert_spaces: true,
    };
    let edits = code_intel.format_code(format_request).await?;
    println!("Format would make {} changes", edits.len());
    
    // 7. Preview rename operation
    let rename_request = RenameSymbolRequest {
        file_path: file_path.to_path_buf(),
        start_row: 8,
        start_column: 4,
        new_name: "new_name".to_string(),
        dry_run: true,
    };
    if let Some(workspace_edit) = code_intel.rename_symbol(rename_request).await? {
        let change_count = workspace_edit.changes
            .as_ref()
            .map(|c| c.values().map(|v| v.len()).sum::<usize>())
            .unwrap_or(0);
        println!("Rename would make {} changes", change_count);
    }
    
    // 8. Get diagnostics
    let diagnostics = code_intel.get_diagnostics(file_path).await?;
    println!("Found {} diagnostics", diagnostics.len());
    
    // 9. Close file
    code_intel.close_file(file_path).await?;
    
    Ok(())
}
```

## Error Handling

All APIs return `Result<T>` with `anyhow::Error`. Common error scenarios:

- **Language server not found**: Install required LSP server
- **File not found**: Ensure file paths are correct and accessible
- **LSP communication failure**: Language server crashed or incompatible
- **Invalid position**: Line/column out of bounds
- **Unsupported operation**: Some LSP servers don't support all features

```rust
match code_intel.goto_definition(file_path, line, col).await {
    Ok(Some(definition)) => println!("Found definition"),
    Ok(None) => println!("No definition found"),
    Err(e) => eprintln!("Error: {}", e),
}
```

## Language Server Requirements

- **TypeScript/JavaScript**: `npm install -g typescript-language-server typescript`
- **Rust**: `rustup component add rust-analyzer`
- **Python**: `pip install python-lsp-server`

## Performance Considerations

- **Initialization**: First request per language server may be slower
- **File watching**: Language servers may watch filesystem for changes
- **Concurrent operations**: Library supports multiple concurrent requests
- **Memory usage**: Each language server runs as separate process
- **Caching**: Language servers cache analysis results

## Thread Safety

`CodeIntelligence` is **not** thread-safe. Use separate instances per thread or wrap in `Arc<Mutex<>>` for shared access.

```rust
use std::sync::{Arc, Mutex};

let code_intel = Arc::new(Mutex::new(CodeIntelligence::with_rust()));
let code_intel_clone = code_intel.clone();

tokio::spawn(async move {
    let mut ci = code_intel_clone.lock().unwrap();
    // Use ci...
});
```
