---
doc_meta:
  validated: 2026-01-05
  commit: a1d370b5
  status: validated
  testable_headless: true
  category: settings-group
  title: Knowledge Base Settings
  description: Settings for knowledge base functionality and indexing
  keywords: [settings, knowledge, base, indexing, patterns]
---

# Knowledge Base Settings

Configure knowledge base functionality, file indexing, and processing parameters.

## knowledge.defaultIncludePatterns

Default file patterns to include in knowledge base.

### Overview

Specifies which file patterns to include when indexing files for the knowledge base. Uses glob patterns to match files.

### Usage

```bash
kiro-cli settings knowledge.defaultIncludePatterns '["*.py", "*.js", "*.md"]'
```

**Type**: Array of strings  
**Default**: `["*.md", "*.txt", "*.py", "*.js", "*.ts"]`

### Pattern Examples

- `"*.py"` - Python files
- `"*.js"` - JavaScript files  
- `"docs/**/*.md"` - Markdown files in docs directory
- `"src/**/*"` - All files in src directory

---

## knowledge.defaultExcludePatterns

Default file patterns to exclude from knowledge base.

### Overview

Specifies which file patterns to exclude when indexing files for the knowledge base. Useful for ignoring build artifacts, dependencies, and temporary files.

### Usage

```bash
kiro-cli settings knowledge.defaultExcludePatterns '["*.log", "node_modules", ".git"]'
```

**Type**: Array of strings  
**Default**: `["node_modules", ".git", "*.log", "*.tmp"]`

### Common Exclusions

- `"node_modules"` - Node.js dependencies
- `".git"` - Git repository data
- `"*.log"` - Log files
- `"build/"` - Build output
- `"__pycache__"` - Python cache
- `"*.pyc"` - Python bytecode

---

## knowledge.maxFiles

Maximum number of files to index in knowledge base.

### Overview

Sets the maximum number of files that can be indexed in the knowledge base to prevent excessive memory usage and processing time.

### Usage

```bash
kiro-cli settings knowledge.maxFiles 1000
```

**Type**: Number  
**Default**: `1000`

### Considerations

**Higher Values**:
- More comprehensive knowledge base
- Increased memory usage
- Longer indexing time

**Lower Values**:
- Faster indexing
- Lower memory usage
- May miss important files

---

## knowledge.chunkSize

Text chunk size for knowledge base processing.

### Overview

Controls the size of text chunks when processing files for the knowledge base. Smaller chunks provide more precise retrieval but may lose context, while larger chunks preserve context but may be less precise.

### Usage

```bash
kiro-cli settings knowledge.chunkSize 512
```

**Type**: Number  
**Default**: `512`  
**Unit**: Characters

### Chunk Size Guidelines

- **256**: Very precise, may lose context
- **512**: Balanced (default)
- **1024**: Better context, less precise
- **2048**: Maximum context, lowest precision

---

## knowledge.chunkOverlap

Overlap between text chunks in knowledge base.

### Overview

Sets the number of characters that overlap between adjacent text chunks. Overlap helps preserve context across chunk boundaries and improves retrieval accuracy.

### Usage

```bash
kiro-cli settings knowledge.chunkOverlap 50
```

**Type**: Number  
**Default**: `50`  
**Unit**: Characters

### Overlap Guidelines

- **0**: No overlap, maximum efficiency
- **25**: Minimal overlap
- **50**: Balanced (default)
- **100**: High overlap, best context preservation

---

## knowledge.indexType

Type of knowledge base index to use.

### Overview

Specifies the indexing algorithm for the knowledge base. Different index types offer different trade-offs between speed, accuracy, and memory usage.

### Usage

```bash
kiro-cli settings knowledge.indexType "fast"
```

**Type**: String  
**Default**: `"balanced"`  
**Values**: `"fast"`, `"balanced"`, `"accurate"`

### Index Types

**fast**:
- Fastest retrieval
- Lower memory usage
- Reduced accuracy

**balanced** (default):
- Good speed/accuracy balance
- Moderate memory usage
- Recommended for most use cases

**accurate**:
- Highest accuracy
- Slower retrieval
- Higher memory usage