---
doc_meta:
  validated: 2025-12-19
  commit: 57090ffe
  status: validated
  testable_headless: false
  category: slash_command
  title: /paste
  description: Paste image from system clipboard into conversation for vision model analysis
  keywords: [paste, clipboard, image, screenshot, vision]
  related: [fs-read]
---

# /paste

Paste image from system clipboard into conversation for vision model analysis.

## Overview

The `/paste` command pastes an image from the system clipboard and sends it to the AI for analysis. Useful for pasting screenshots, diagrams, or images copied from other applications. Requires vision-capable model.

## Usage

```
/paste
```

Pastes image from clipboard and sends to AI.

## How It Works

1. Reads image from system clipboard
2. Saves image to temporary file
3. Sends file path to AI as input
4. AI analyzes image (if vision-capable model)

## Examples

### Example 1: Paste Screenshot

1. Take screenshot (Cmd+Shift+4 on macOS)
2. Screenshot automatically copied to clipboard
3. In chat: `/paste`
4. AI analyzes screenshot

### Example 2: Paste Diagram

1. Copy diagram image from browser/app
2. In chat: `/paste`
3. Ask: "Explain this architecture diagram"

### Example 3: Paste Error Screenshot

1. Copy error dialog screenshot
2. In chat: `/paste`
3. AI reads error from image

## Troubleshooting

### Issue: "Failed to paste image"

**Symptom**: Error message when pasting  
**Causes**:
- Clipboard is empty
- Clipboard contains text, not image
- Clipboard access denied
- Unsupported image format

**Solution**: 
- Ensure image is copied to clipboard
- Check clipboard contains image, not text
- Grant clipboard permissions to terminal

### Issue: AI Can't See Image

**Symptom**: AI says it can't see image  
**Cause**: Model doesn't support vision  
**Solution**: Switch to vision-capable model (Claude 3.5 Sonnet, Claude 3 Opus)

### Issue: Image Quality Poor

**Symptom**: AI misreads image content  
**Cause**: Low resolution or unclear image  
**Solution**: Use higher resolution screenshot or clearer image

## Related Features

- [fs_read Image mode](../tools/fs-read.md) - Read image files directly
- [/editor](editor.md) - Compose text prompts
- [/model](model.md) - Switch to vision-capable model

## Limitations

- Images only (not text)
- Requires clipboard access
- Not available in headless mode
- Platform-dependent clipboard support
- Requires vision-capable model
- Supported formats: PNG, JPG, JPEG, GIF, WEBP

## Technical Details

**Clipboard**: Uses system clipboard API to read image data

**Platforms**: macOS, Linux, Windows

**Image Handling**: Saves to temporary file, passes path to AI

**Vision Models**: Claude 3.5 Sonnet, Claude 3 Opus, Claude 3 Haiku support images

**File Cleanup**: Temporary image files managed by system
