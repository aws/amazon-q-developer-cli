# Amazon Q CLI Automatic Naming Feature - User Guide

## Overview

The Automatic Naming feature for Amazon Q CLI allows you to save conversations without manually specifying filenames. The system intelligently analyzes your conversation content and generates meaningful, consistent filenames based on the topics discussed.

## Basic Usage

### Saving a Conversation

To save a conversation with an automatically generated filename:

```
/save
```

This will save your conversation to the default location (`~/qChats/`) with an automatically generated filename in the format:

```
Q_[MainTopic]_[SubTopic]_[ActionType] - DDMMMYY-HHMM.q.json
```

For example: `Q_AmazonQ_CLI_FeatureRequest - 04JUL25-1600.q.json`

### Saving to a Custom Directory

To save a conversation to a specific directory:

```
/save /path/to/directory/
```

Note the trailing slash (`/`) which indicates you want to save to a directory with an auto-generated filename.

### Saving with a Specific Filename (Backward Compatibility)

To save a conversation with a specific filename:

```
/save /path/to/file.q.json
```

## Advanced Features

### Using Templates

You can save conversations using predefined templates:

```
/save --template technical
```

This will use the "technical" template for generating the filename.

### Using Configuration Settings

To use your current configuration settings for generating the filename:

```
/save --config
```

### Adding Metadata

You can add metadata to the saved conversation:

```
/save --metadata category=work,priority=high
```

### Security Options

#### Redacting Sensitive Information

To redact sensitive information (credit card numbers, API keys, etc.) from the saved conversation:

```
/save --redact
```

#### Preventing Overwriting

To prevent overwriting existing files:

```
/save --no-overwrite
```

#### Following Symlinks

To follow symlinks when saving:

```
/save --follow-symlinks
```

#### Setting File Permissions

To set custom file permissions:

```
/save --file-permissions 644
```

#### Setting Directory Permissions

To set custom directory permissions:

```
/save --dir-permissions 755
```

## Configuration

### Setting the Default Save Path

To set the default path for saving conversations:

```
q settings set save.default_path /path/to/directory
```

### Setting the Default Filename Format

To set the default filename format:

```
q settings set save.filename_format default
```

Or for a custom format:

```
q settings set save.filename_format "custom:{main_topic}-{date}"
```

Available placeholders:
- `{main_topic}`: Main topic extracted from conversation
- `{sub_topic}`: Sub-topic extracted from conversation
- `{action_type}`: Action type extracted from conversation
- `{date}`: Date in the configured format
- `{id}`: Conversation ID

### Setting the Prefix

To set the prefix for filenames:

```
q settings set save.prefix "Chat_"
```

### Setting the Separator

To set the separator for filename components:

```
q settings set save.separator "-"
```

### Setting the Date Format

To set the date format:

```
q settings set save.date_format "YYYY-MM-DD"
```

Available date formats:
- `DDMMMYY-HHMM`: Default format (e.g., `04JUL25-1600`)
- `YYYY-MM-DD`: ISO format (e.g., `2025-07-04`)
- `MM-DD-YYYY`: US format (e.g., `07-04-2025`)
- `DD-MM-YYYY`: European format (e.g., `04-07-2025`)
- `YYYY/MM/DD`: Alternative format (e.g., `2025/07/04`)

### Setting the Topic Extractor

To set the topic extractor:

```
q settings set save.topic_extractor_name "advanced"
```

Available topic extractors:
- `basic`: Simple keyword-based extraction
- `enhanced`: Improved extraction with better context awareness
- `advanced`: Sophisticated extraction with NLP techniques

### Creating Templates

To create a template:

```
q settings set save.templates.technical "Tech_{main_topic}_{date}"
```

### Setting Security Options

To set security options:

```
q settings set save.redact_sensitive true
q settings set save.prevent_overwrite true
q settings set save.follow_symlinks false
q settings set save.file_permissions 600
q settings set save.directory_permissions 700
```

## Examples

### Basic Save

```
/save
```

Saves the conversation to `~/qChats/Q_AmazonQ_CLI_Help - 04JUL25-1600.q.json`

### Save to Custom Directory

```
/save ~/Documents/Conversations/
```

Saves the conversation to `~/Documents/Conversations/Q_AmazonQ_CLI_Help - 04JUL25-1600.q.json`

### Save with Template and Metadata

```
/save --template technical --metadata category=work,priority=high
```

Saves the conversation using the "technical" template and adds metadata.

### Save with Redaction and No Overwrite

```
/save --redact --no-overwrite
```

Saves the conversation with sensitive information redacted and prevents overwriting existing files.

## Troubleshooting

### File Permission Issues

If you encounter permission issues when saving:

1. Check that you have write permissions for the target directory
2. Try saving to a different location
3. Check if the file is being used by another process

### Path Too Deep

If you receive a "Path too deep" error:

1. Try saving to a location with a shorter path
2. Increase the maximum path depth in the configuration

### Invalid Path

If you receive an "Invalid path" error:

1. Check that the path does not contain invalid characters
2. Ensure the path is properly formatted

### File Already Exists

If you receive a "File already exists" error:

1. Use the `--no-overwrite` option to generate a unique filename
2. Specify a different filename

## Best Practices

1. **Use the default automatic naming** for most cases
2. **Create templates** for different types of conversations
3. **Enable redaction** when saving conversations with sensitive information
4. **Set appropriate file permissions** to protect your data
5. **Use metadata** to organize and categorize your saved conversations
