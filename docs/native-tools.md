# Native tools

- [`execute_bash`](#the_execute_bash_tool) — Execute a shell command.
- [`fs_read`](#the_fs_read_tool) — Read files, directories, and images.
- [`fs_write`](#the-fs-write-tool) — Create and edit files.
- [`gh_issue`](#the-gh-issue-tool) — Open a GitHub issue template.
- [`knowledge`](#the-knowledge-tool) — Store and retrieve information in a knowledge base.
- [`thinking`](#the-thinking-tool) — Internal reasoning mechanism.
- [`use_aws`](#the-use-aws-tool) — Make AWS CLI API calls.

### The `execute_bash` tool

Execute the specified bash command.

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
{
  "allowedCommands": ["git status", "git fetch"],
  "allowReadOnly": true
}
```

### The `fs_read` tool

Tool for reading files, directories and images.

#### Schema

```json
{
  "type": "object",
  "properties": {
    "allowedPaths": {
      "type": "array",
      "items": {
        "type": "string"
      },
      "default": []
    }
  }
}
```

#### Example

```json
{
  "allowedPaths": ["~"]
}
```

### The `fs_write` tool

Tool for creating and editing files.

#### Schema

```json
{
  "type": "object",
  "properties": {
    "allowedPaths": {
      "type": "array",
      "items": {
        "type": "string"
      },
      "default": []
    }
  }
}
```

#### Example

```json
{
  "allowedPaths": [
    "~/file-to-create.txt",
    "~/editable-file.txt",
    "~/my-workspace/"
  ]
}
```

### The `gh_issue` tool

Opens the browser to our GitHub template for reporting issues with `q`.

This tool has no configuration.

### The `knowledge` tool

Store and retrieve information in knowledge base across chat sessions

This tool has no configuration.

### The `thinking` tool

Thinking is an internal reasoning mechanism improving the quality of complex tasks by breaking their atomic actions down.

This tool has no configuration.

### The `use_aws` tool

Make an AWS CLI api call with the specified service, operation, and parameters.

#### Schema

```json
{
  "type": "object",
  "properties": {
    "allowedServices": {
      "type": "array",
      "items": {
        "type": "string"
      },
      "default": []
    }
  }
}
```

#### Example

```json
{
  "allowedServices": ["s3", "iam"]
}
```
