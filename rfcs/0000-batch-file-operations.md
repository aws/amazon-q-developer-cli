# Safety Features

To ensure safe and reliable file operations, especially when modifying multiple files or making multiple edits to a single file, we propose the following safety features:

## Content Hash Verification

For line-based operations like `replace_lines` and `insert`, we will require a hash of the source content to verify that the file hasn't been modified since it was last read:

```json
{
  "name": "fs_write",
  "parameters": {
    "command": "replace_lines",
    "fileEdits": [
      {
        "path": "/path/to/file.txt",
        "edits": [
          {
            "command": "replace_lines",
            "start_line": 10,
            "end_line": 15,
            "new_str": "This content replaces lines 10 through 15",
            "content_hash": "a1b2c3d4e5f6..." // Hash of the original content from lines 10-15
          }
        ]
      }
    ]
  }
}
```

If the content at the specified line range has changed since it was read (hash doesn't match), the operation will fail with an appropriate error message. This prevents unintended modifications when the file has been changed by another process between reading and writing.

## Backup Creation

For operations that modify existing files, an optional `create_backup` parameter can be provided to create a backup of the original file before making changes:

```json
{
  "name": "fs_write",
  "parameters": {
    "command": "pattern_replace",
    "directory": "/path/to/project",
    "file_pattern": "*.js",
    "sed_pattern": "s/const /let /g",
    "create_backup": true,
    "backup_extension": ".bak"
  }
}
```

This allows for easy recovery if the changes need to be reverted.

## Dry Run Mode

A `dry_run` parameter can be provided to preview the changes that would be made without actually modifying any files:

```json
{
  "name": "fs_write",
  "parameters": {
    "command": "pattern_replace",
    "directory": "/path/to/project",
    "file_pattern": "*.js",
    "sed_pattern": "s/const /let /g",
    "dry_run": true
  }
}
```

The response will include the files that would be modified and the changes that would be made, allowing users to verify the changes before applying them.
