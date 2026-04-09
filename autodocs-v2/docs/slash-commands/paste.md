---
doc_meta:
  validated: 2026-04-09
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
2. Converts image to PNG format in memory
3. Encodes image as base64
4. Sends base64 data to AI for analysis

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

### Issue: "No image in clipboard"

**Symptom**: Error message when pasting  
**Causes**:
- Clipboard is empty
- Clipboard contains text, not image

**Solution**: Ensure image is copied to clipboard

### Issue: "Failed to access clipboard"

**Symptom**: Clipboard access error  
**Cause**: Clipboard permissions denied or unavailable  
**Solution**: Grant clipboard permissions to terminal

### Issue: "Unsupported image format"

**Symptom**: Format conversion error  
**Cause**: Clipboard image format not supported  
**Solution**: Copy image in a standard format

### Issue: AI Can't See Image

**Symptom**: AI says it can't see image  
**Cause**: Model doesn't support vision  
**Solution**: Switch to vision-capable model (Claude 3.5 Sonnet, Claude 3 Opus)

## Related Features

- [fs_read Image mode](../tools/fs-read.md) - Read image files directly
- [/reply](reply.md) - Compose a reply
- [/model](model.md) - Switch to vision-capable model

## Limitations

- Images only (not text)
- Requires clipboard access
- Not available in headless mode
- Platform-dependent clipboard support
- Requires vision-capable model

## Technical Details

**Clipboard**: Uses system clipboard API (arboard) to read image data

**Platforms**: macOS, Linux, Windows

**Image Handling**: Converts to PNG in memory, encodes as base64, sends inline to AI

**Output Format**: Always PNG (converted internally regardless of source format)

**Vision Models**: Claude 3.5 Sonnet, Claude 3 Opus, Claude 3 Haiku support images
