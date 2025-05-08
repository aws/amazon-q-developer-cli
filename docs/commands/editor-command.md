# Editor Command

## Overview
The editor command opens an external text editor for composing longer or more complex prompts for Amazon Q.

## Command Details
- **Name**: `editor`
- **Description**: Open an external editor for composing prompts
- **Usage**: `/editor [initial_text]`
- **Requires Confirmation**: No

## Functionality
The editor command allows you to compose longer or more complex prompts in your preferred text editor. When you run the command, it opens your system's default text editor (as specified by the EDITOR environment variable) with optional initial text. After you save and close the editor, the content is sent as a prompt to Amazon Q.

This is particularly useful for:
- Multi-paragraph prompts
- Code snippets with proper formatting
- Complex instructions that benefit from careful editing
- Prompts that include special characters or formatting

## Example Usage
```
/editor
```

This opens your default text editor with an empty buffer. After you write your prompt, save the file, and close the editor, the content is sent to Amazon Q.

```
/editor Please help me with the following code:
```

This opens your default text editor with the initial text "Please help me with the following code:". You can then add your code and additional instructions before sending.

## Related Commands
- `/ask`: Send a prompt directly without using an editor
- `/compact`: Summarize conversation history

## Use Cases
- Writing detailed technical questions
- Including code snippets with proper indentation
- Composing multi-part prompts with structured sections
- Carefully editing prompts before sending them

## Notes
- The editor command uses your system's default text editor (EDITOR environment variable)
- Common editors include vim, nano, emacs, VS Code, etc.
- You can set your preferred editor by configuring the EDITOR environment variable
- The command supports optional initial text that will be pre-populated in the editor
- All content from the editor is sent as a single prompt to Amazon Q
