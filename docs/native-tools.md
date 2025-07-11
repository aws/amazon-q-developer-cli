## execute_bash

Execute the specified bash command.

### Config

#### Schema

```json
{
  "type": "object",
  "properties": {
    "allowedCommands": {
      "type": "array",
      "items": {
        "type": "string"
      },
      "default": []
    },
    "allowReadOnly": {
      "type": "boolean",
      "default": true
    }
  }
}
```

#### Example

```json
{}
```

## fs_read

Tool for reading files, directories and images. Always provide an 'operations' array.

For single operation: provide array with one element.
For batch operations: provide array with multiple elements.

Available modes:

- Line: Read lines from a file
- Directory: List directory contents
- Search: Search for patterns in files
- Image: Read and process image

## fs_write

## gh_issue

## knowledge

## thinking

## use_aws
