# File Versioning and Chunk Management

To support efficient management of file content in conversation history, we propose adding versioning information to the fs_read response:

## Content Hash and Last Modified Timestamp

Each successful fs_read operation will include:
- A `content_hash` of the file or chunk being read
- A `last_modified` timestamp in UTC format

```json
{
  "path": "/path/to/file.txt",
  "success": true,
  "content": "File content here...",
  "content_hash": "a1b2c3d4e5f6...",
  "last_modified": "2025-05-11T10:15:30Z"
}
```

## Benefits for Conversation History Management

This versioning information enables:

1. **Chunk Consolidation**: Multiple chunks from the same file with identical `last_modified` timestamps can be consolidated in conversation history
2. **Version Tracking**: Changes to files can be tracked across multiple reads
3. **Stale Content Detection**: Older chunks with outdated `last_modified` timestamps can be identified
4. **Efficient Disposal**: Outdated chunks can be safely removed from conversation history
5. **Content Verification**: The `content_hash` can be used to verify file integrity

## Implementation Approach

- Use standard file system metadata to obtain `last_modified` timestamps
- Generate `content_hash` using a fast hashing algorithm (e.g., xxHash or Blake3)
- Include versioning information in all fs_read responses, both single file and batch operations
