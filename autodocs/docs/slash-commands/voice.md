---
doc_meta:
  title: /voice
  description: Start voice input mode for hands-free speech-to-text prompts
  category: slash_command
  keywords: [voice, speech, microphone, recording, transcription, whisper, dictation]
  related: [voice-mode, voice-settings]
  validated: 2026-06-22
  commit: 4dbb4e51f
  status: validated
  testable_headless: false
---

# /voice

Start voice input mode for hands-free speech-to-text prompts.

## Overview

The `/voice` command activates voice recording. Speak your prompt, and it will be transcribed and submitted to the AI assistant. Recording stops when you press Enter or after a period of silence.

## Usage

```
/voice
/voice --continuous
/voice -c
/voice --model local-whisper
/voice --model remote
```

## Options

| Option | Short | Description |
|--------|-------|-------------|
| `--continuous` | `-c` | Enable continuous voice mode (auto-record after each response) |
| `--model` | | Transcription backend: `local-whisper` (default) or `remote` |

## Keybind

Press `Ctrl+O` from the chat prompt to instantly start voice recording without typing the command.

**Push-to-Talk**: Hold `Space` for 1.5 seconds to activate push-to-talk mode. Release to stop recording and transcribe.

## Behavior

### Single Recording

```
> /voice
Recording, press ENTER when done... 2.1sec ░░░████████

"Fix the authentication bug"

> Analyzing the authentication...
```

1. Recording starts immediately
2. Voice activity bar shows audio level
3. Press Enter or wait for silence to stop
4. Transcription is auto-submitted

### Continuous Mode

```
> /voice --continuous
Continuous voice mode enabled

Recording, press ENTER when done...
```

In continuous mode:
- Recording automatically starts after each assistant response
- Toggle off by running `/voice --continuous` again
- Or press `Ctrl+C` to exit

## Examples

### Basic Voice Input

```
> /voice
Recording, press ENTER when done... 1.5sec ░░░██████

"List all files in the current directory"

> Here are the files...
```

### Continuous Conversation

```
> /voice -c
Continuous voice mode enabled

Recording, press ENTER when done... 2.0sec ░░░████████

"What does the main function do"

> The main function...

Recording, press ENTER when done... 1.8sec ░░░██████

"Show me the error handling"

> Here's the error handling code...

> /voice -c
Continuous voice mode disabled
```

### Force Remote Backend

```
> /voice --model remote
Recording via remote server...
```

Use `--model remote` to explicitly use the remote voice server instead of local Whisper.

### Push-to-Talk with Ctrl+O

From the prompt, press `Ctrl+O`:

```
> [Ctrl+O pressed]
Recording, press ENTER when done... 3.2sec ░░░██████████

"Refactor this function to use async await"

> I'll refactor the function...
```

### Push-to-Talk with Space Hold

Hold the `Space` key for 1.5 seconds to activate push-to-talk:

```
> [Space held for 1.5s]
Recording... release Space to stop

"Add error handling to this function"

> Adding error handling...
```

## Configuration

Voice behavior is controlled by settings:

```bash
# Adjust silence timeout (seconds before auto-stop)
kiro-cli settings set voice.silenceTimeout 8

# Change Whisper model size
kiro-cli settings set voice.modelSize small

# Set transcription language
kiro-cli settings set voice.language en
```

See [Voice Settings](../settings/voice-settings.md) for all options.

## Troubleshooting

### Command not available

If you don't see `/voice` in the command list, ensure you're running the latest version:

```bash
kiro-cli --version
```

### "No microphone detected"

Voice mode requires a microphone. On cloud desktops, configure a remote voice server:

```bash
kiro-cli settings set voice.serverUrl http://localhost:19876
```

### Recording stops too quickly

Increase the silence timeout:

```bash
kiro-cli settings set voice.silenceTimeout 10
```

### Poor transcription quality

Try a larger Whisper model:

```bash
kiro-cli settings set voice.modelSize small
```

### First use is slow

The Whisper model downloads on first use (~75MB-1.5GB depending on size). Subsequent uses are instant.

## Limitations

- Requires microphone access (or remote voice server)
- Transcription is CPU-intensive
- English-optimized by default
- No edit/review before submission (auto-submits)

## Related

- [Voice Mode](../features/voice-mode.md) - Full feature documentation
- [Voice Settings](../settings/voice-settings.md) - Configuration options
