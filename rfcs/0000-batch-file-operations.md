- Feature Name: batch_file_operations
- Start Date: 2025-05-11

# Summary

[summary]: #summary

Enhance the fs_read and fs_write tools to support batch operations on multiple files in a single call, with the ability to perform multiple edits per file, maintain line number integrity through proper edit ordering, and perform search/replace operations across files in a folder using wildcard patterns with sed-like syntax.

# Implementation Staging

To ensure a smooth and manageable implementation process, we propose breaking down the work into three distinct phases:

## Phase 1: fs_read Batch Operations

The first phase will focus on enhancing the fs_read tool to support reading multiple files in a single operation:

- Add the `paths` parameter to fs_read
- Implement batch processing logic for multiple files
- Update the response format to handle multiple file results
- Add comprehensive error handling for batch operations
- Add tests for the new functionality

This phase provides immediate value by allowing users to read multiple files in a single operation, which is a common use case.

## Phase 2: Pattern Replacement for fs_write

The second phase will add the pattern-based search and replace functionality to fs_write:

- Add the `pattern_replace` command to fs_write
- Integrate the sd crate for sed-like functionality
- Implement file pattern matching with glob/globset
- Add support for recursive directory traversal
- Add tests for pattern replacement functionality

This phase adds powerful search and replace capabilities across multiple files, addressing the need for sed-like functionality in a safer and more controlled manner.

## Phase 3: Multi-File Operations for fs_write

The final phase will complete the batch operations feature by adding support for multiple edits across multiple files:

- Add the `fileEdits` parameter to fs_write
- Implement edit ordering logic for maintaining line number integrity
- Add the `replace_lines` command
- Update the response format to handle multiple file results with detailed error reporting
- Add tests for multi-file operations and multiple edits per file

This phase completes the feature by enabling complex file modifications across multiple files in a single operation.

Each phase will be implemented and tested independently, allowing for incremental delivery of value to users.

# Motivation

[motivation]: #motivation

Currently, Amazon Q CLI's fs_read and fs_write tools can only operate on one file at a time. This creates inefficiency when users need to perform the same operation on multiple files or make multiple edits to a single file, requiring multiple separate tool calls. This leads to:

1. Verbose and repetitive code in Amazon Q responses
2. Slower execution due to multiple tool invocations
3. More complex error handling across multiple calls
4. Difficulty in maintaining atomicity across related file operations
