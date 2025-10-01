# File-Based Prompts

File-based prompts allow you to create reusable templates with dynamic arguments. They are stored as `.md` files in your prompts directories.

## Prompt Locations

- **Global**: `~/.aws/amazonq/prompts/`
- **Local**: `.amazonq/prompts/` (project-specific)

## Creating Prompts

### Simple Prompts with Auto-Detection

The easiest way to create a prompt is to simply use `{{placeholder}}` syntax in your markdown file:

```markdown
Get current weather for {{city}} in {{units}} format.

Please include temperature, humidity, and forecast.
```

**Arguments are automatically detected:**
- `city` (optional)
- `units` (optional)

### Prompts with YAML Frontmatter (Optional)

You can optionally add YAML frontmatter for descriptions and explicit argument definitions:

```markdown
---
description: "Get weather information for a city"
arguments:
  - name: "city"
    description: "City name to get weather for"
    required: true
  - name: "units"
    description: "Temperature units (celsius/fahrenheit)"
    required: false
---

Get current weather for {{city}} in {{units}} format.

Please include temperature, humidity, and forecast.
```

### Description-Only Frontmatter

You can provide just a description and let arguments be auto-detected:

```markdown
---
description: "Get current date in specified format"
---

Today's date in {{format}} format is: {{date}}

Please use {{timezone}} timezone for the calculation.
```

**Result:**
- Description: "Get current date in specified format"
- Auto-detected arguments: `format`, `date`, `timezone` (all optional)

## Argument Auto-Detection

The system automatically detects `{{placeholder}}` patterns in your prompt content:

- **Pattern**: `{{word}}` (word characters only: letters, numbers, underscore)
- **Default**: All auto-detected arguments are optional
- **No duplicates**: Each unique placeholder becomes one argument

## Using Prompts

### List Available Prompts
```
/prompts
```

### Use a Prompt
```
@prompt-name arg1="value1" arg2="value2"
```

### Get Prompt Details
```
/prompts details prompt-name
```

## Examples

### 1. Simple Code Generation
**File**: `generate-function.md`
```markdown
Write a {{language}} function named {{function_name}} that {{description}}.

Include proper error handling and documentation.
```

**Usage**: `@generate-function language="Python" function_name="calculate_tax" description="calculates tax based on income"`

### 2. Documentation Template
**File**: `api-docs.md`
```markdown
---
description: "Generate API documentation"
---

# {{api_name}} API Documentation

## Endpoint: {{endpoint}}
**Method**: {{method}}

### Description
{{description}}

### Parameters
{{parameters}}

### Response Format
{{response_format}}
```

### 3. Plain Text Prompt
**File**: `summarize.md`
```markdown
Please summarize the following {{content_type}} in {{length}} sentences:

{{content}}

Focus on the main points and key takeaways.
```

## Best Practices

1. **Use descriptive placeholder names**: `{{user_name}}` instead of `{{x}}`
2. **Add descriptions for complex prompts**: Use YAML frontmatter when helpful
3. **Keep prompts focused**: One clear purpose per prompt
4. **Test your prompts**: Verify they work with different argument values
5. **Use consistent naming**: Follow a naming convention for your prompts

## Migration from Old Format

Existing prompts with YAML frontmatter continue to work unchanged. You can:

1. **Keep existing prompts as-is** - they work perfectly
2. **Simplify prompts** - remove YAML frontmatter if you only need auto-detection
3. **Mix approaches** - use frontmatter for some prompts, auto-detection for others
