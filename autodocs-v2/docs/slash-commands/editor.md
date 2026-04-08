---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: slash_command
  title: /editor
  description: Open $EDITOR to compose multi-line prompts with optional initial text
  keywords: [editor, compose, multiline, vim, text]
  related: [reply, paste]
---

# /editor

Open $EDITOR to compose multi-line prompts with optional initial text.

## Overview

Opens your default editor ($EDITOR, defaults to vi) to compose multi-line prompts. Creates temporary markdown file, opens in editor, and sends content as prompt after save. Useful for complex queries, code snippets, or formatted text.

## Usage

### Basic Usage

```
/editor
```

Opens empty editor.

### With Initial Text

```
/editor Explain this code:
```

Opens editor pre-filled with "Explain this code:".

## How It Works

1. Creates temporary `.md` file in system temp directory
2. Writes initial text if provided
3. Opens file in $EDITOR
4. Waits for editor to close
5. Reads content from file
6. Sends as prompt (if not empty)
7. Cleans up temporary file

## Examples

### Example 1: Compose Long Prompt

```
/editor
```

Editor opens. Write multi-line prompt, save, exit. Content sent to AI.

### Example 2: Start with Template

```
/editor Review this code for:
```

Editor opens with "Review this code for:" already written. Add details, save, exit.

### Example 3: Empty Content

```
/editor
```

Write nothing, save, exit.

**Output**:
```
⚠ Empty content from editor, not submitting.
```

## Editor Configuration

### Set Editor

```bash
export EDITOR=vim
export EDITOR=nano
export EDITOR="code --wait"
export EDITOR="emacs -nw"
```

### Default

If $EDITOR not set, uses `vi`.

## Troubleshooting

### Issue: Wrong Editor Opens

**Symptom**: Unexpected editor launches  
**Cause**: $EDITOR set to different editor  
**Solution**: Check `echo $EDITOR`. Set to preferred editor.

### Issue: Editor Doesn't Wait

**Symptom**: Content not captured  
**Cause**: Editor doesn't block (e.g., `code` without `--wait`)  
**Solution**: Add wait flag: `export EDITOR="code --wait"`

### Issue: Can't Save

**Symptom**: Error saving file  
**Cause**: Permission issue with temp directory  
**Solution**: Check temp directory permissions

### Issue: Content Not Sent

**Symptom**: Editor closes but nothing happens  
**Cause**: File was empty  
**Solution**: Write content before saving

## Related Features

- [/reply](reply.md) - Open editor with assistant message quoted
- [/paste](paste.md) - Paste image from clipboard
- [/prompts](prompts.md) - Manage reusable prompts

## Limitations

- Requires $EDITOR or vi installed
- Not available in headless mode
- Temporary file in system temp directory
- No auto-save or recovery
- Editor must block (wait for close)

## Technical Details

**Temp File**: `q_prompt_<uuid>.md` in system temp directory

**Editor Command**: Parsed with shlex to support arguments (e.g., "code --wait")

**Cleanup**: Temporary file deleted after reading content

**Empty Check**: Trims whitespace. Empty content not submitted.

**History**: Content added to input history (accessible with up arrow)

**Display**: Content echoed to terminal before sending to AI
