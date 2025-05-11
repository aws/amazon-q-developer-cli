- Feature Name: batch_file_operations
- Start Date: 2025-05-11

# Summary

[summary]: #summary

Enhance the fs_read and fs_write tools to support batch operations on multiple files in a single call, with the ability to perform multiple edits per file and maintain line number integrity through proper edit ordering.

# Motivation

[motivation]: #motivation

Currently, Amazon Q CLI's fs_read and fs_write tools can only operate on one file at a time. This creates inefficiency when users need to perform the same operation on multiple files or make multiple edits to a single file, requiring multiple separate tool calls. This leads to:

1. Verbose and repetitive code in Amazon Q responses
2. Slower execution due to multiple tool invocations
3. More complex error handling across multiple calls
4. Difficulty in maintaining atomicity across related file operations

Users commonly need to:
- Read multiple configuration files at once
- Write to multiple output files in a single operation
- Perform the same text replacement across multiple files
- Create multiple related files as part of a single logical operation
- Make multiple edits to a single file while maintaining line number integrity

By enhancing these tools to support batch operations, we can significantly improve the efficiency and user experience of the Amazon Q CLI.

# Guide-level explanation

[guide-level-explanation]: #guide-level-explanation

## Reading Multiple Files

With the enhanced fs_read tool, you can now read multiple files in a single operation:

```javascript
// Before: Reading files one at a time
const file1Content = await fs_read({ mode: "Line", path: "/path/to/file1.txt" });
const file2Content = await fs_read({ mode: "Line", path: "/path/to/file2.txt" });

// After: Reading multiple files at once
const results = await fs_read({
  mode: "Line",
  paths: ["/path/to/file1.txt", "/path/to/file2.txt", "/path/to/file3.txt"]
});

// Results will be an array of objects with path, success, and content properties
results.forEach(result => {
  if (result.success) {
    console.log(`Content of ${result.path}: ${result.content}`);
  } else {
    console.log(`Failed to read ${result.path}: ${result.error}`);
  }
});
```

## Writing to Multiple Files with Multiple Edits

The enhanced fs_write tool allows you to perform multiple operations on multiple files:

```javascript
// Before: Writing to files one at a time
await fs_write({
  command: "create",
  path: "/path/to/file1.txt",
  file_text: "Hello, world!"
});
await fs_write({
  command: "create",
  path: "/path/to/file2.txt",
  file_text: "Another file"
});

// After: Writing to multiple files at once
await fs_write({
  command: "create",
  fileEdits: [
    {
      path: "/path/to/file1.txt",
      edits: [
        {
          command: "create",
          file_text: "Hello, world!"
        }
      ]
    },
    {
      path: "/path/to/file2.txt",
      edits: [
        {
          command: "create",
          file_text: "Another file"
        }
      ]
    }
  ]
});
```

## Multiple Edits to a Single File

You can now make multiple edits to a single file in one operation:

```javascript
await fs_write({
  command: "str_replace",
  fileEdits: [
    {
      path: "/path/to/config.json",
      edits: [
        {
          command: "str_replace",
          old_str: "\"debug\": false",
          new_str: "\"debug\": true"
        },
        {
          command: "str_replace",
          old_str: "\"version\": \"1.0.0\"",
          new_str: "\"version\": \"1.1.0\""
        },
        {
          command: "insert",
          insert_line: 5,
          new_str: "  \"newSetting\": \"value\","
        }
      ]
    }
  ]
});
```

## New replace_lines Command

The new replace_lines command allows replacing a range of lines in a file:

```javascript
await fs_write({
  command: "replace_lines",
  fileEdits: [
    {
      path: "/path/to/file.txt",
      edits: [
        {
          command: "replace_lines",
          start_line: 10,
          end_line: 15,
          new_str: "This content replaces lines 10 through 15"
        }
      ]
    }
  ]
});
```

## Error Handling

The batch operations provide detailed error reporting:

```javascript
const results = await fs_write({
  command: "str_replace",
  fileEdits: [/* ... */]
});

// Check results for each file
results.forEach(result => {
  if (result.success) {
    console.log(`Successfully edited ${result.path}: ${result.edits_applied} edits applied`);
  } else {
    console.log(`Failed to edit ${result.path}: ${result.error}`);
    if (result.failed_edits) {
      result.failed_edits.forEach(failedEdit => {
        console.log(`  Failed edit (${failedEdit.command}): ${failedEdit.error}`);
      });
    }
  }
});
```

# Reference-level explanation

[reference-level-explanation]: #reference-level-explanation

## API Changes

### fs_read Enhancements

```json
{
  "description": "Tool for reading files, directories and images. Now supports batch operations.",
  "name": "fs_read",
  "parameters": {
    "properties": {
      "path": {
        "description": "Path to the file or directory. The path should be absolute, or otherwise start with ~ for the user's home.",
        "type": "string"
      },
      "paths": {
        "description": "Array of paths to read. Each path should be absolute, or otherwise start with ~ for the user's home.",
        "type": "array",
        "items": {
          "type": "string"
        }
      },
      "mode": {
        "description": "The mode to run in: `Line`, `Directory`, `Search`, `Image`.",
        "enum": ["Line", "Directory", "Search", "Image"],
        "type": "string"
      },
      // Other existing parameters remain unchanged
    },
    "required": ["mode"],
    "oneOf": [
      { "required": ["path"] },
      { "required": ["paths"] }
    ],
    "type": "object"
  }
}
```

### fs_write Enhancements

```json
{
  "description": "A tool for creating and editing files. Now supports batch operations with multiple edits per file.",
  "name": "fs_write",
  "parameters": {
    "properties": {
      "command": {
        "description": "The commands to run. Allowed options are: `create`, `str_replace`, `insert`, `append`, `replace_lines`.",
        "enum": ["create", "str_replace", "insert", "append", "replace_lines"],
        "type": "string"
      },
      "path": {
        "description": "Absolute path to file or directory, e.g. `/repo/file.py` or `/repo`.",
        "type": "string"
      },
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
                    "description": "The command for this edit. Allowed options are: `create`, `str_replace`, `insert`, `append`, `replace_lines`.",
                    "enum": ["create", "str_replace", "insert", "append", "replace_lines"],
                    "type": "string"
                  },
                  "file_text": {
                    "description": "Required parameter of `create` command, with the content of the file to be created.",
                    "type": "string"
                  },
                  "old_str": {
                    "description": "Required parameter of `str_replace` command containing the string in `path` to replace.",
                    "type": "string"
                  },
                  "new_str": {
                    "description": "Required parameter of `str_replace`, `insert`, `append`, and `replace_lines` commands containing the new string.",
                    "type": "string"
                  },
                  "insert_line": {
                    "description": "Required parameter of `insert` command. The `new_str` will be inserted AFTER the line `insert_line` of `path`.",
                    "type": "integer"
                  },
                  "start_line": {
                    "description": "Required parameter of `replace_lines` command. The starting line number to replace (inclusive).",
                    "type": "integer"
                  },
                  "end_line": {
                    "description": "Required parameter of `replace_lines` command. The ending line number to replace (inclusive).",
                    "type": "integer"
                  }
                },
                "required": ["command"],
                "allOf": [
                  {
                    "if": {
                      "properties": { "command": { "enum": ["create"] } }
                    },
                    "then": {
                      "required": ["file_text"]
                    }
                  },
                  {
                    "if": {
                      "properties": { "command": { "enum": ["str_replace"] } }
                    },
                    "then": {
                      "required": ["old_str", "new_str"]
                    }
                  },
                  {
                    "if": {
                      "properties": { "command": { "enum": ["insert"] } }
                    },
                    "then": {
                      "required": ["insert_line", "new_str"]
                    }
                  },
                  {
                    "if": {
                      "properties": { "command": { "enum": ["append"] } }
                    },
                    "then": {
                      "required": ["new_str"]
                    }
                  },
                  {
                    "if": {
                      "properties": { "command": { "enum": ["replace_lines"] } }
                    },
                    "then": {
                      "required": ["start_line", "end_line", "new_str"]
                    }
                  }
                ]
              }
            }
          },
          "required": ["path", "edits"]
        }
      },
      // Other existing parameters remain unchanged
    },
    "required": ["command"],
    "oneOf": [
      { "required": ["path"] },
      { "required": ["fileEdits"] }
    ],
    "type": "object"
  }
}
```

## Response Format

### fs_read Response

For single file operations (using `path`), the response format remains unchanged.

For batch operations (using `paths`), the response will be an array of results:

```json
[
  {
    "path": "/path/to/file1.txt",
    "success": true,
    "content": "File content here..."
  },
  {
    "path": "/path/to/file2.txt",
    "success": false,
    "error": "File not found"
  }
]
```

### fs_write Response

For single file operations (using `path`), the response format remains unchanged.

For batch operations (using `fileEdits`), the response will be an array of results:

```json
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
```

## Implementation Details

### Edit Application Order

For multiple edits on a single file, edits will be applied from the end of the file to the beginning to avoid line number issues. This is implemented by:

1. Sorting edits by line number in descending order
2. For commands without line numbers (like `str_replace`), they will be applied after line-based edits
3. For `append` operations, they will always be applied last

```rust
// Pseudocode for sorting edits
fn sort_edits(edits: &mut Vec<Edit>) {
    edits.sort_by(|a, b| {
        let a_line = get_effective_line_number(a);
        let b_line = get_effective_line_number(b);
        b_line.cmp(&a_line) // Descending order
    });
}

fn get_effective_line_number(edit: &Edit) -> i32 {
    match edit.command.as_str() {
        "insert" => edit.insert_line.unwrap_or(0),
        "replace_lines" => edit.start_line.unwrap_or(0),
        "append" => i32::MAX, // Append always goes at the end
        _ => 0, // Other commands don't have line numbers
    }
}
```

### Error Handling

Batch operations will continue processing all files even if some operations fail. For each file, the implementation will:

1. Track the number of successful and failed edits
2. Collect detailed error information for each failed edit
3. Continue processing remaining edits even if some fail
4. Return a comprehensive result object with success/failure information

## New replace_lines Command

The new `replace_lines` command allows replacing a range of lines in a file:

1. Takes `start_line`, `end_line`, and `new_str` parameters
2. Replaces all lines from `start_line` to `end_line` (inclusive) with the content in `new_str`
3. Line numbers are 0-based (first line is line 0)

Implementation pseudocode:

```rust
fn replace_lines(path: &str, start_line: usize, end_line: usize, new_str: &str) -> Result<()> {
    let content = fs::read_to_string(path)?;
    let lines: Vec<&str> = content.lines().collect();
    
    if start_line >= lines.len() || end_line >= lines.len() || start_line > end_line {
        return Err(Error::new("Invalid line range"));
    }
    
    let mut new_lines = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if i < start_line || i > end_line {
            new_lines.push(*line);
        } else if i == start_line {
            new_lines.push(new_str);
        }
    }
    
    let new_content = new_lines.join("\n");
    fs::write(path, new_content)?;
    Ok(())
}
```

# Drawbacks

[drawbacks]: #drawbacks

1. **Increased Complexity**: The enhanced tools have more complex parameter schemas and response formats, which may make them slightly harder to understand for new users.

2. **Potential for Misuse**: Batch operations could be misused to perform too many operations at once, potentially causing performance issues.

3. **Error Handling Complexity**: With multiple operations in a single call, error handling becomes more complex, as some operations may succeed while others fail.

4. **Implementation Effort**: The changes require significant modifications to the existing tools, including new parameter parsing, response formatting, and edit ordering logic.

# Rationale and alternatives

[rationale-and-alternatives]: #rationale-and-alternatives

## Why This Design?

1. **Extending Existing Tools**: We chose to extend the existing tools rather than create new ones to maintain a consistent API and avoid tool proliferation.

2. **Multiple Edits Per File**: Supporting multiple edits per file in a single operation allows for more complex file modifications while maintaining atomicity.

3. **Edit Ordering**: Applying edits from the end of the file to the beginning ensures that line numbers remain valid throughout the edit process, avoiding common issues with sequential edits.

4. **New replace_lines Command**: Adding a dedicated command for replacing line ranges is more efficient and less error-prone than using multiple individual line edits.

## Alternatives Considered

1. **New Batch Tools**: We could create new tools specifically for batch operations (e.g., `fs_read_batch` and `fs_write_batch`). This would keep the existing tools simpler but would introduce redundancy and require users to learn new tools.

2. **Smart Parameter Detection**: We could modify the existing tools to detect parameter types automatically (e.g., if `path` is an array, treat it as a batch operation). This would be more concise but could lead to confusion and unexpected behavior.

3. **No Edit Ordering**: We could leave it to the user to order edits correctly. This would simplify the implementation but would make the tool more error-prone and harder to use correctly.

4. **No Multiple Edits Per File**: We could support batch operations on multiple files but not multiple edits per file. This would be simpler but would still require multiple tool calls for complex file modifications.

## Impact of Not Doing This

If we don't implement batch file operations:

1. Users will continue to need multiple tool calls for common operations, leading to verbose and repetitive code.
2. Performance will be suboptimal due to the overhead of multiple tool invocations.
3. Error handling will remain complex across multiple calls.
4. Atomicity of related file operations will be difficult to maintain.
5. Line number issues will continue to be a common source of errors when making multiple edits to a file.

# Unresolved questions

[unresolved-questions]: #unresolved-questions

1. **Throttling for Large Batches**: Should we implement throttling or limits for large batch operations to prevent performance issues?

2. **Dependencies Between File Operations**: How should we handle dependencies between file operations? For example, if one file operation depends on the success of another.

3. **Continue on Error Flag**: Should we add a "continue on error" flag to control whether batch operations should continue processing remaining files if some operations fail?

4. **Dry Run Option**: Would it be valuable to add a "dry run" option to preview changes without applying them?

5. **Backward Compatibility Edge Cases**: Are there any edge cases where the new batch operations might behave differently from multiple single operations?

# Future possibilities

[future-possibilities]: #future-possibilities

1. **Transaction Support**: Add support for transactional file operations, where all operations either succeed or fail as a unit.

2. **Conditional Edits**: Allow edits to be conditional based on file content or the success of previous edits.

3. **Pattern-Based Edits**: Support pattern matching (e.g., regular expressions) for more flexible file modifications.

4. **Diff Preview**: Add the ability to preview the changes that would be made by a batch operation before applying them.

5. **Undo Support**: Implement the ability to undo batch operations by automatically creating backups.

6. **Progress Reporting**: For large batch operations, provide progress updates during execution.

7. **Parallel Processing**: Implement parallel processing for independent file operations to improve performance.

8. **Integration with Version Control**: Add awareness of version control systems to handle file modifications more intelligently.
