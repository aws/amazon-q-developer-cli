---
doc_meta:
  title: Voice Settings
  description: Configuration options for voice input mode including model size, timeouts, and language
  category: setting
  keywords: [voice, settings, whisper, model, timeout, language, transcription, microphone]
  related: [voice-mode, voice-command]
  validated: 2026-06-22
  commit: 4dbb4e51f
  status: validated
  testable_headless: true
---

# Voice Settings

Configuration options for voice input mode.

## Overview

Voice settings control how voice input behaves, including the transcription model, recording timeouts, and language. All settings are optional—voice mode works with sensible defaults.

## Settings Reference

### voice.language

Voice input language for transcription.

| Property | Value |
|----------|-------|
| Type | string |
| Default | `en` |
| Scope | global |

```bash
# Get current value
kiro-cli settings get voice.language

# Set to Spanish
kiro-cli settings set voice.language es

# Reset to default
kiro-cli settings reset voice.language
```

### voice.modelSize

Whisper model size for local transcription. Larger models are more accurate but slower.

| Property | Value |
|----------|-------|
| Type | string |
| Default | `base` |
| Scope | global |

Available models:

| Model | Size | Speed | Accuracy |
|-------|------|-------|----------|
| `tiny` | ~75MB | Fastest | Lower |
| `base` | ~142MB | Fast | Good |
| `small` | ~466MB | Medium | Better |
| `medium` | ~1.5GB | Slow | High |
| `turbo` | ~1.62GB | Fast | High |
| `turbo-q5` | ~574MB | Fast | High |

```bash
# Use smaller model for speed
kiro-cli settings set voice.modelSize tiny

# Use larger model for accuracy
kiro-cli settings set voice.modelSize small

# Use turbo for best speed/accuracy balance
kiro-cli settings set voice.modelSize turbo
```

### voice.silenceTimeout

Seconds of silence before recording automatically stops.

| Property | Value |
|----------|-------|
| Type | integer |
| Default | `5` |
| Scope | global |

```bash
# Increase for slower speakers
kiro-cli settings set voice.silenceTimeout 10

# Decrease for faster auto-stop
kiro-cli settings set voice.silenceTimeout 3
```

### voice.maxSessionTime

Maximum recording duration in seconds. Recording stops automatically after this time.

| Property | Value |
|----------|-------|
| Type | integer |
| Default | `300` |
| Scope | global |

```bash
# Allow longer recordings
kiro-cli settings set voice.maxSessionTime 600

# Limit to 1 minute
kiro-cli settings set voice.maxSessionTime 60
```

### voice.serverUrl

URL of a remote voice server for cloud desktop use. When set, voice mode falls back to this server if no local microphone is detected.

| Property | Value |
|----------|-------|
| Type | string |
| Default | none |
| Scope | global |

```bash
# Configure remote voice server
kiro-cli settings set voice.serverUrl http://localhost:19876

# Clear remote server
kiro-cli settings reset voice.serverUrl
```

### voice.autoSubmit

Whether to automatically submit transcribed text without review.

| Property | Value |
|----------|-------|
| Type | boolean |
| Default | `true` |
| Scope | global, session-safe |

```bash
# Disable auto-submit (review before sending)
kiro-cli settings set voice.autoSubmit false
```

## Examples

### Optimize for Speed

```bash
kiro-cli settings set voice.modelSize tiny
kiro-cli settings set voice.silenceTimeout 3
```

### Optimize for Accuracy

```bash
kiro-cli settings set voice.modelSize small
kiro-cli settings set voice.silenceTimeout 8
```

### Cloud Desktop Setup

```bash
# On cloud desktop, point to local voice server
kiro-cli settings set voice.serverUrl http://localhost:19876
```

### View All Voice Settings

```bash
kiro-cli settings list | grep voice
```

## Troubleshooting

### Model download takes too long

Use a smaller model:

```bash
kiro-cli settings set voice.modelSize tiny
```

### Recording stops too early

Increase silence timeout:

```bash
kiro-cli settings set voice.silenceTimeout 10
```

### Transcription is inaccurate

Try a larger model:

```bash
kiro-cli settings set voice.modelSize small
```

Or use the turbo model for good accuracy with reasonable speed:

```bash
kiro-cli settings set voice.modelSize turbo
```

## Related

- [Voice Mode](../features/voice-mode.md) - Full feature documentation
- [/voice command](../slash-commands/voice.md) - Slash command reference
