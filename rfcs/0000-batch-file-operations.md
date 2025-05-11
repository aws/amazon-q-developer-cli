# Current and Proposed Schemas

## Current Schemas

### fs_read Input Schema

```json
{
  "description": "Tool for reading files, directories and images.",
  "name": "fs_read",
  "parameters": {
    "properties": {
      "context_lines": {
        "default": 2,
        "description": "Number of context lines around search results (optional, for Search mode)",
        "type": "integer"
      },
      "depth": {
        "description": "Depth of a recursive directory listing (optional, for Directory mode)",
        "type": "integer"
      },
      "end_line": {
        "default": -1,
        "description": "Ending line number (optional, for Line mode). A negative index represents a line number starting from the end of the file.",
        "type": "integer"
      },
      "image_paths": {
        "description": "List of paths to the images. This is currently supported by the Image mode.",
        "items": {
          "type": "string"
        },
        "type": "array"
      },
      "mode": {
        "description": "The mode to run in: `Line`, `Directory`, `Search`, `Image`.",
        "enum": ["Line", "Directory", "Search", "Image"],
        "type": "string"
      },
      "path": {
        "description": "Path to the file or directory. The path should be absolute, or otherwise start with ~ for the user's home.",
        "type": "string"
      },
      "pattern": {
        "description": "Pattern to search for (required, for Search mode). Case insensitive. The pattern matching is performed per line.",
        "type": "string"
      },
      "start_line": {
        "default": 1,
        "description": "Starting line number (optional, for Line mode). A negative index represents a line number starting from the end of the file.",
        "type": "integer"
      }
    },
    "required": ["path", "mode"],
    "type": "object"
  }
}
```

### fs_read Output Schema

```json
// Line Mode Success
{
  "path": "/path/to/file.txt",
  "success": true,
  "content": "The content of the file or specified lines"
}

// Directory Mode Success
{
  "path": "/path/to/directory",
  "success": true,
  "content": "total 123\ndrwxr-xr-x  user group  4096 May 11 10:15 .\n..."
}

// Search Mode Success
{
  "path": "/path/to/file.txt",
  "success": true,
  "content": "Line 10: matching content\nLine 11: more matching content\n..."
}

// Error Case (for any mode)
{
  "path": "/path/to/file.txt",
  "success": false,
  "error": "Error message describing what went wrong"
}
```

### fs_write Input Schema

```json
{
  "description": "A tool for creating and editing files",
  "name": "fs_write",
  "parameters": {
    "properties": {
      "command": {
        "description": "The commands to run. Allowed options are: `create`, `str_replace`, `insert`, `append`.",
        "enum": ["create", "str_replace", "insert", "append"],
        "type": "string"
      },
      "file_text": {
        "description": "Required parameter of `create` command, with the content of the file to be created.",
        "type": "string"
      },
      "insert_line": {
        "description": "Required parameter of `insert` command. The `new_str` will be inserted AFTER the line `insert_line` of `path`.",
        "type": "integer"
      },
      "new_str": {
        "description": "Required parameter of `str_replace`, `insert`, and `append` commands: new content.",
        "type": "string"
      },
      "old_str": {
        "description": "Required parameter of `str_replace` command containing the string in `path` to replace.",
        "type": "string"
      },
      "path": {
        "description": "Absolute path to file or directory, e.g. `/repo/file.py` or `/repo`.",
        "type": "string"
      }
    },
    "required": ["command", "path"],
    "type": "object"
  }
}
```

### fs_write Output Schema

```json
// Success Case
{
  "path": "/path/to/file.txt",
  "success": true
}

// Error Case
{
  "path": "/path/to/file.txt",
  "success": false,
  "error": "Error message describing what went wrong"
}
```

## Proposed Schema Additions

### fs_read Input Schema Additions

```json
{
  "parameters": {
    "properties": {
      // Existing properties remain unchanged
      "paths": {
        "description": "Array of paths to read. Each path should be absolute, or otherwise start with ~ for the user's home.",
        "type": "array",
        "items": {
          "type": "string"
        }
      }
    },
    "required": ["mode"],
    "oneOf": [
      { "required": ["path"] },
      { "required": ["paths"] }
    ]
  }
}
```

### fs_read Output Schema Additions

```json
// Single File Success with Versioning
{
  "path": "/path/to/file.txt",
  "success": true,
  "content": "The content of the file or specified lines",
  "content_hash": "a1b2c3d4e5f6...",
  "last_modified": "2025-05-11T10:15:30Z"
}

// Batch Operation Success
[
  {
    "path": "/path/to/file1.txt",
    "success": true,
    "content": "File content here...",
    "content_hash": "a1b2c3d4e5f6...",
    "last_modified": "2025-05-11T10:15:30Z"
  },
  {
    "path": "/path/to/file2.txt",
    "success": false,
    "error": "File not found"
  }
]
```

### fs_write Input Schema Additions

```json
{
  "parameters": {
    "properties": {
      "command": {
        "description": "The commands to run. Allowed options are: `create`, `str_replace`, `insert`, `append`, `replace_lines`, `pattern_replace`.",
        "enum": ["create", "str_replace", "insert", "append", "replace_lines", "pattern_replace"],
        "type": "string"
      },
      // Existing properties remain unchanged
      "fileEdits": {
        "description": "Array of file edit operations to perform in batch. Each object must include path and an array of edits to apply to that file.",
        "type": "array",
        "items": {
          "type": "object",
          "properties": {
            "path": {
              "description": "Absolute path to file, e.g. `/repo/file.py`.",
              "type": "string"
            },
            "edits": {
              "description": "Array of edit operations to apply to this file. Edits will be applied from the end of the file to the beginning to avoid line number issues.",
              "type": "array",
              "items": {
                "type": "object",
                "properties": {
                  "command": {
                    "description": "The command for this edit.",
                    "enum": ["create", "str_replace", "insert", "append", "replace_lines"],
                    "type": "string"
                  },
                  // Other properties similar to the main fs_write parameters
                  "content_hash": {
                    "description": "Hash of the original content for line-based operations. Required for replace_lines and insert commands to verify file hasn't changed.",
                    "type": "string"
                  }
                }
              }
            }
          },
          "required": ["path", "edits"]
        }
      },
      "directory": {
        "description": "Directory to search for files matching the pattern. Required for pattern_replace command.",
        "type": "string"
      },
      "file_pattern": {
        "description": "Glob pattern to match files for pattern_replace command (e.g., '*.js', '**/*.py').",
        "type": "string"
      },
      "sed_pattern": {
        "description": "Sed-like pattern for search and replace (e.g., 's/search/replace/g'). Required for pattern_replace command.",
        "type": "string"
      },
      "recursive": {
        "description": "Whether to search recursively in subdirectories for pattern_replace command.",
        "type": "boolean"
      },
      "exclude_patterns": {
        "description": "Array of glob patterns to exclude from pattern_replace command.",
        "type": "array",
        "items": {
          "type": "string"
        }
      },
      "dry_run": {
        "description": "Preview changes without modifying files.",
        "type": "boolean"
      }
    },
    "required": ["command"],
    "oneOf": [
      { "required": ["path"] },
      { "required": ["fileEdits"] },
      { 
        "allOf": [
          { "required": ["directory", "file_pattern", "sed_pattern"] },
          { "properties": { "command": { "enum": ["pattern_replace"] } } }
        ]
      }
    ]
  }
}
```

### fs_write Output Schema Additions

```json
// Batch Operation Success
[
  {
    "path": "/path/to/file1.txt",
    "success": true,
    "edits_applied": 3,
    "edits_failed": 0
  },
  {
    "path": "/path/to/file2.txt",
    "success": false,
    "error": "Permission denied",
    "edits_applied": 0,
    "edits_failed": 2,
    "failed_edits": [
      {
        "command": "str_replace",
        "error": "String not found in file"
      },
      {
        "command": "insert",
        "error": "Line number out of range"
      }
    ]
  }
]

// Pattern Replace Success
{
  "success": true,
  "files_modified": 5,
  "files_skipped": 2,
  "files": [
    {
      "path": "/path/to/file1.js",
      "success": true,
      "replacements": 10
    },
    {
      "path": "/path/to/file2.js",
      "success": false,
      "error": "Permission denied"
    }
  ]
}

// Dry Run Result
{
  "success": true,
  "dry_run": true,
  "files": [
    {
      "path": "/path/to/file1.js",
      "would_modify": true,
      "replacements": 10,
      "preview": "--- Original\n+++ Modified\n@@ -10,7 +10,7 @@\n-const x = 5;\n+let x = 5;"
    }
  ]
}
```
