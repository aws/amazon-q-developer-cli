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
{
  "allowedCommands": ["git status", "git fetch"],
  "allowReadOnly": true
}
```

## fs_read

Tool for reading files, directories and images.

### Config

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

## fs_write

Tool for creating and editing files.

### Config

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

## gh_issue

Opens the browser to our GitHub template for reporting issues with `q`.

### Config

This tool has no configuration.

## knowledge

Store and retrieve information in knowledge base across chat sessions

### Config

This tool has no configuration.

## thinking

Thinking is an internal reasoning mechanism improving the quality of complex tasks by breaking their atomic actions down.

### Config

This tool has no configuration.

## use_aws

Make an AWS CLI api call with the specified service, operation, and parameters.

### Config

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
