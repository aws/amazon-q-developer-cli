# Adding a New Language LSP

## Before JSON Configuration (5 places to change)
Previously, adding a new language required changes in 5 different places in the code.

## After JSON Configuration (1 place to change)

To add a new language LSP, simply add an entry to `config/languages.json`:

```json
{
  "languages": {
    "go": {
      "name": "gopls",
      "command": "gopls",
      "args": [],
      "file_extensions": ["go"],
      "initialization_options": {}
    }
  }
}
```

## Example: Adding Java Support

```json
{
  "languages": {
    "java": {
      "name": "jdtls",
      "command": "jdtls",
      "args": ["-data", "/tmp/jdtls-workspace"],
      "file_extensions": ["java"],
      "initialization_options": {
        "settings": {
          "java": {
            "configuration": {
              "runtimes": []
            }
          }
        }
      }
    }
  }
}
```

## Configuration Fields

- **`name`**: Unique identifier for the language server
- **`command`**: Executable command to start the LSP server
- **`args`**: Command line arguments for the LSP server
- **`file_extensions`**: File extensions this language handles (without dots)
- **`initialization_options`**: LSP initialization options (JSON object)

## That's It!

No code changes required. The system will automatically:
- ✅ Detect the language based on file extensions
- ✅ Start the appropriate LSP server
- ✅ Handle all LSP operations for the new language
- ✅ Include it in workspace detection
- ✅ Support all code intelligence features

## Testing

Add a test project for the new language in the regression tests by creating a `create_<language>_project` method in the test files.
